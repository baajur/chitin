use crate::Request;
use regex::Regex;

pub fn gen_enum_json(prev: &[String], params: &[Request]) -> String {
    if let Some(cur) = prev.first() {
        let inner = gen_enum_json(&prev[1..prev.len()], params);
        format!("{{ \"{}\": {} }}", cur, inner)
    } else {
        let inner = params
            .iter()
            .map(|r| r.name.clone())
            .collect::<Vec<_>>()
            .join(", ");
        format!("{{ {} }}", inner)
    }
}

pub fn to_typescript_type(path: &str) -> String {
    let re = Regex::new(r"\w*::").unwrap();
    let result = re.replace_all(path, "");
    let re = Regex::new(r"Vec").unwrap();
    let result = re.replace_all(result.as_ref(), "Array");
    let re = Regex::new(r"\(\)").unwrap();
    let result = re.replace_all(result.as_ref(), "null");
    let re = Regex::new(r"(usize|i32|u32|i64|u64|f32|f64)").unwrap();
    let result = re.replace_all(result.as_ref(), "number");
    let re = Regex::new(r"String").unwrap();
    let result = re.replace_all(result.as_ref(), "string");
    // 處理 tuple
    let re = Regex::new(r"\(").unwrap();
    let result = re.replace_all(result.as_ref(), "[");
    let re = Regex::new(r"\)").unwrap();
    let mut result = re.replace_all(result.as_ref(), "]");
    // 處理 Option
    // TODO: 處理多層 Option 與 tuple
    let re = Regex::new(r"^Option<(.+)>$").unwrap();
    if let Some(caps) = re.captures(result.as_ref()) {
        if let Some(inner) = caps.get(1) {
            result = std::borrow::Cow::Owned(format!("{}?", inner.as_str()));
        }
    }
    // TODO: 其它基礎型別的轉換
    result.to_owned().to_string()
}
