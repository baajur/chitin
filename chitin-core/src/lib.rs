use inflector::Inflector;
use proc_macro2::TokenStream;
use quote::{quote, ToTokens};
use regex::Regex;

#[derive(Clone, Debug)]
pub enum CodegenOption {
    Server,
    Client { is_root: bool },
}
impl CodegenOption {
    pub fn is_server(&self) -> bool {
        match self {
            Self::Server => true,
            _ => false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    pub ty: String,
    pub name: String,
}
impl ToTokens for Request {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let ty = &self.ty;
        let name = &self.name;
        tokens.extend(quote! {
            Request {
                ty: #ty.to_owned(),
                name: #name.to_owned(),
            }
        });
    }
}

fn to_typescript_type(path: &str) -> String {
    let re = Regex::new(r"\w*::").unwrap();
    let result = re.replace_all(path, "");
    let re = Regex::new(r"Vec").unwrap();
    let result = re.replace_all(result.as_ref(), "Array");
    let re = Regex::new(r"\(\)").unwrap();
    let result = re.replace_all(result.as_ref(), "void");
    // TODO: 其它基礎型別的轉換
    result.to_owned().to_string()
}

fn gen_arg_string(requests: &[Request], with_type: bool, opt: &CodegenOption) -> String {
    if requests.len() == 0 {
        "".to_owned()
    } else {
        let mut args = if with_type {
            format!("{}: {}", requests[0].name, requests[0].ty)
        } else {
            format!("{}", requests[0].name)
        };
        for req in &requests[1..] {
            if with_type {
                let ty = if opt.is_server() {
                    req.ty.clone()
                } else {
                    to_typescript_type(&req.ty)
                };
                args.push_str(&format!(", {}: {}", req.name, ty));
            } else {
                args.push_str(&format!(", {}", req.name));
            }
        }
        args
    }
}

pub enum FuncOrCode {
    Func(fn(&CodegenOption) -> String),
    Code(TokenStream),
}
impl std::fmt::Debug for FuncOrCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        match self {
            FuncOrCode::Code(code) => {
                write!(f, "Code({})", code.to_string())?;
            }
            FuncOrCode::Func(_) => {
                write!(f, "Function")?;
            }
        }
        Ok(())
    }
}
impl FuncOrCode {
    fn gen_string(&self, opt: &CodegenOption) -> String {
        match self {
            FuncOrCode::Func(f) => f(opt),
            _ => panic!(),
        }
    }
}

#[derive(Debug)]
pub enum Entry {
    Leaf {
        name: String,
        response_ty: String,
        request: Vec<Request>,
    },
    Node {
        name: String,
        query_name: String,
        codegen: FuncOrCode,
    },
}

impl ToTokens for Entry {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        match self {
            Entry::Node {
                name,
                query_name,
                codegen,
            } => {
                if let FuncOrCode::Code(code) = codegen {
                    tokens.extend(quote! {
                        Entry::Node {
                            name: #name.to_owned(),
                            query_name: #query_name.to_owned(),
                            codegen: FuncOrCode::Func(#code)
                        }
                    });
                } else {
                    panic!("內部實作錯誤")
                }
            }
            Entry::Leaf {
                name,
                response_ty,
                request,
            } => {
                let request = request.iter();
                tokens.extend(quote! {
                    Entry::Leaf {
                        name: #name.to_owned(),
                        response_ty: #response_ty.to_owned(),
                        request: vec![#(#request),*]
                    }
                });
            }
        }
    }
}

pub trait ChitinCodegen {
    fn get_name() -> &'static str;
    fn get_entries() -> Vec<Entry>;
    fn codegen(opt: &CodegenOption) -> String {
        if opt.is_server() {
            Self::server_codegen(opt)
        } else {
            Self::client_codegen(opt)
        }
    }
    fn client_codegen(opt: &CodegenOption) -> String {
        let (is_root, leaf_opt) = {
            let mut t = opt.clone();
            if let CodegenOption::Client { ref mut is_root } = t {
                if *is_root {
                    *is_root = false;
                    (true, t)
                } else {
                    (false, t)
                }
            } else {
                panic!()
            }
        };
        let mut code = "".to_owned();
        if is_root {
            code.push_str(&format!("abstract class {}Fetcher {{\n", Self::get_name()));
            code.push_str("    abstract fetchResult(query: String): Promise<String>;\n");
        }
        for entry in Self::get_entries().iter() {
            match entry {
                Entry::Leaf {
                    name,
                    response_ty,
                    request,
                } => {
                    code.push_str(&format!(
                        "    async {}({}): Promise<{}> {{\n",
                        get_query_func_name(name),
                        gen_arg_string(request, true, opt),
                        to_typescript_type(response_ty)
                    ));
                    // TODO: fetchResult 的參數要怎麼塞？
                    code.push_str("        return JSON.parse(await this.fetchResult());\n");
                    code.push_str("    }\n");
                }
                Entry::Node { codegen, .. } => code.push_str(&codegen.gen_string(&leaf_opt)),
            }
        }
        if is_root {
            code.push_str("}\n");
        }
        code
    }
    fn server_codegen(opt: &CodegenOption) -> String {
        let entries = Self::get_entries();
        let mut routers_name = vec![];
        let mut code = "".to_owned();
        for entry in entries.iter() {
            if let Entry::Node {
                query_name,
                codegen,
                ..
            } = entry
            {
                routers_name.push(get_router_name(query_name));
                code.push_str(&codegen.gen_string(opt));
            }
        }

        code.push_str(&format!(
            "#[async_trait]\ntrait {} {{\n",
            get_router_name(&Self::get_name())
        ));
        for router_name in routers_name.iter() {
            code.push_str(&format!(
                "    type {}: {} + Sync;\n",
                router_name, router_name
            ));
        }
        for entry in entries.iter() {
            match entry {
                Entry::Leaf {
                    name,
                    response_ty,
                    request,
                } => {
                    code.push_str(&format!(
                        "    async fn {}(&self, {}) -> {};\n",
                        get_handler_name(name, false),
                        gen_arg_string(request, true, opt),
                        response_ty
                    ));
                }
                Entry::Node {
                    name, query_name, ..
                } => {
                    code.push_str(&format!(
                        "    fn {}(&self) -> &Self::{};\n",
                        get_handler_name(name, true),
                        get_router_name(query_name)
                    ));
                }
            }
        }
        code.push_str(&format!(
            "    async fn handle(&self, query: {}) -> Result<String, Error> {{\n",
            Self::get_name()
        ));
        code.push_str("        match query {\n");
        for entry in entries.iter() {
            match entry {
                Entry::Leaf { name, request, .. } => {
                    code.push_str(&format!(
                        "             {}::{} {{ {} }} => {{\n",
                        Self::get_name(),
                        name,
                        gen_arg_string(request, false, opt)
                    ));
                    code.push_str(&format!(
                        "                 let resp = self.{}({}).await;\n",
                        get_handler_name(name, false),
                        gen_arg_string(request, false, opt)
                    ));
                    code.push_str(&format!("                 serde_json::to_string(&resp)\n",));
                }
                Entry::Node { name, .. } => {
                    code.push_str(&format!(
                        "             {}::{}(query) => {{\n",
                        Self::get_name(),
                        name
                    ));
                    code.push_str(&format!(
                        "                 self.{}().handle(query).await\n",
                        get_handler_name(name, true)
                    ));
                }
            }
            code.push_str("             }\n");
        }
        code.push_str("        }\n");
        code.push_str("    }\n");
        code.push_str("}\n");
        code
    }
}

pub fn get_router_name(query_name: &str) -> String {
    format!("{}Router", query_name)
}

pub fn get_handler_name(name: &str, is_router: bool) -> String {
    if is_router {
        format!("{}_router", name.to_snake_case())
    } else {
        name.to_snake_case()
    }
}

pub fn get_query_func_name(query_name: &str) -> String {
    query_name.to_camel_case()
}
