use maud::{html, Markup, DOCTYPE};
use rocket::get;

pub fn head(page_title: &str) -> Markup {
    html! {
        (DOCTYPE)
        meta charset="UTF-8";
        meta name="viewport" content="width=device-width, initial-scale=1";
        title { (page_title) }
        link rel="icon" type="image/svg+xml" href="favicon.svg";
        link rel="stylesheet" href="./main.css";
    }
}

pub fn footer() -> Markup {
    html! {
        footer {
            p {a href="/" {"Home"}}
            p {a href="https://github.com/G2-Games/confetti-box" {"Source"}}
            p {a href="https://g2games.dev/" {"My Website"}}
            p {a href="api_info" {"API Info"}}
            p {a href="https://ko-fi.com/g2_games" {"Donate"}}
        }
    }
}

#[get("/api_info")]
pub fn api_info() -> Markup {
    html! {
        (head("Confetti-Box | API"))

        center {
            h1 { "API Information" }
            hr;

            div style="text-align: left;" {
                p {
                    """
                    The API for this service can be used by POST ing a form
                    with an expiration time and file to upload to the upload
                    endpoint:
                    """
                }
                pre { "/upload POST duration=\"6h\" fileUpload=(file data)" }

            }

            hr;
            (footer())
        }
    }
}
