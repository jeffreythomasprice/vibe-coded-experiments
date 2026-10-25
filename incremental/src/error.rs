use wasm_bindgen::JsValue;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("no global window")]
    NoWindow,
    #[error("window has no document")]
    NoDocument,
    #[error("element #{0} not found")]
    ElementNotFound(&'static str),
    #[error("element #{0} is not an html element")]
    NotAnHtmlElement(&'static str),
    #[error("element #{0} is not a canvas")]
    NotACanvas(&'static str),
    #[error("canvas has no 2d context")]
    NoContext2d,
    #[error("javascript error: {0}")]
    Js(String),
}

impl From<JsValue> for AppError {
    fn from(value: JsValue) -> Self {
        AppError::Js(format!("{value:?}"))
    }
}

impl From<AppError> for JsValue {
    fn from(value: AppError) -> Self {
        JsValue::from_str(&value.to_string())
    }
}
