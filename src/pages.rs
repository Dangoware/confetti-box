use maud::{html, Markup, DOCTYPE};
use rocket::{get, State};

use crate::settings::Settings;

pub fn head(page_title: &str) -> Markup {
    html! {
        (DOCTYPE)
        meta charset="UTF-8";
        meta name="viewport" content="width=device-width, initial-scale=1";
        title { (page_title) }
        link rel="icon" type="image/svg+xml" href="/resources/favicon.svg";
        link rel="stylesheet" href="/resources/main.css";
    }
}

pub fn footer() -> Markup {
    html! {
        footer {
            p {a href="/" {"Home"}}
            p {a href="https://github.com/Dangoware/confetti-box" {"Source"}}
            p {a href="https://g2games.dev/" {"My Website"}}
            p {a href="api" {"API"}}
            p {a href="https://ko-fi.com/g2_games" {"Donate"}}
        }
    }
}

#[get("/api")]
pub fn api_info(settings: &State<Settings>) -> Markup {
    let domain = &settings.server.domain;
    let root = &settings.server.root_path;
    html! {
        (head("Confetti-Box | API"))

        center {
            h1 { "API Information" }
            hr;

            div style="text-align: left;" {
                p {
                    "Confetti-Box is designed to be simple to access using its
                    API. All endpoints are accessed following "
                    code{"https://"(domain) (root)} ". All responses are encoded
                    in JSON. MMIDs are a unique identifier for a file returned by
                    the server after a successful " code{"/upload"} " request."
                }
                p {
                    "The following endpoints are supported:"
                }

                h2 { code {"/upload"} }
                pre { r#"POST duration=String fileUpload=Bytes -> JSON"# }
                p {
                    "To upload files, " code{"POST"} " a multipart form
                    containing the fields " code{"duration"} " and "
                    code{"fileData"} " to this endpoint. " code{"duration"}
                    " MUST be a string formatted like " code{"1H"}", where
                    the number MUST be a valid number and the letter MUST be
                    one of " b{"S"} "(econd), " b{"M"}"(inute), " b{"H"}"(our), "
                    b{"D"}"(ay). The " code{"/info"} " endpoint returns valid
                    durations and maximum file sizes."
                }
                p {
                    "Example successful response:"
                }
                pre {
                    "{\n\t\"status\": true,\n\t\"response\": \"\",\n\t\"name\": \"1600-1200.jpg\",\n\t\"mmid\": \"xNLF6ogx\",\n\t\"hash\": \"1f12137f2c263d9e6d686e90c687a55d46d064fe6eeda7e4c39158d20ce1f071\",\n\t\"expires\": \"2024-10-28T11:59:25.024373438Z\"\n}"
                }
                p {"Example failure response:"}
                pre {
                    "{\n\t\"status\": false,\n\t\"response\": \"Duration invalid\",\n}"
                }

                hr;
                h2 { code {"/info"} }
                pre { r#"GET -> JSON"# }
                p {
                    "Returns the capabilities of the server."
                }
                p {"Example response:"}
                pre {
                    "{\n\t\"max_filesize\": 5000000000,\n\t\"max_duration\": 259200,\n\t\"default_duration\": 21600,\n\t\"allowed_durations\": [\n\t\t3600,\n\t\t21600,\n\t\t86400,\n\t\t172800\n\t]\n}"
                }

                hr;
                h2 { code {"/f/<mmid>"} }
                pre { r#"GET mmid=MMID -> Redirect or File"# }
                p {
                    "By default issues a redirect to the full URL for a file. This
                    behavior can be modified by appending " code{"?noredir"} " to
                    the end of this request, like " code{"/f/<mmid>?noredir"} ",
                    in which case it behaves just like " code{"/f/<mmid>/<filename>"}
                }
                p {"Example default response:"}
                pre {"303: /f/xNLF6ogx/1600-1200.jpg"}

                p {"Example modified response:"}
                pre {"<File Bytes>"}

                hr;
                h2 { code {"/f/<mmid>/<filename>"} }
                pre { r#"GET mmid=MMID filename=String -> File"# }
                p {
                    "Returns the contents of the file corresponding to the
                    requested MMID, but with the corresponding filename so as
                    to preserve it for downloads. Mostly for use by browsers."
                }
                p {"Example response:"}
                pre {
                    "<File Bytes>"
                }
            }

            hr;
            (footer())
        }
    }
}
