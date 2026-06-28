pub(in crate::app) fn default_request_body(method: &str) -> &'static str {
    match method {
        "POST" | "PUT" | "PATCH" => "{\n  \n}",
        _ => "",
    }
}

pub(in crate::app) fn default_body_mode(method: &str) -> &'static str {
    match method {
        "POST" | "PUT" | "PATCH" => "raw",
        _ => "none",
    }
}
