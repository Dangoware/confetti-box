use rocket::{get, http::ContentType, response::content::{RawCss, RawJavaScript}};

#[get("/resources/fonts/<font>")]
pub fn font_static(font: &str) -> Option<(ContentType, &'static [u8])> {
    match font {
        "Roboto.woff2" => Some((ContentType::WOFF2, include_bytes!("../web/fonts/roboto.woff2"))),
        "FiraCode.woff2" => Some((ContentType::WOFF2, include_bytes!("../web/fonts/fira-code.woff2"))),
        _ => None
    }
}

/// Stylesheet
#[get("/resources/main.css")]
pub fn stylesheet() -> RawCss<&'static str> {
    RawCss(include_str!("../web/main.css"))
}

/// Upload handler javascript
#[get("/resources/request.js")]
pub fn form_handler_js() -> RawJavaScript<&'static str> {
    RawJavaScript(include_str!("../web/request.js"))
}

#[get("/resources/favicon.svg")]
pub fn favicon() -> (ContentType, &'static str) {
    (ContentType::SVG, include_str!("../web/favicon.svg"))
}
