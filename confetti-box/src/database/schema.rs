// @generated automatically by Diesel CLI.

diesel::table! {
    mochifiles (mmid) {
        mmid -> Text,
        name -> Text,
        mime_type -> Text,
        hash -> Text,
        upload_datetime -> Timestamp,
        expiry_datetime -> Timestamp,
    }
}
