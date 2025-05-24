// @generated automatically by Diesel CLI.

diesel::table! {
    mochifiles (mmid) {
        mmid -> Nullable<Integer>,
        name -> Text,
        mime_type -> Text,
        hash -> Text,
        upload_datetime -> Nullable<Timestamp>,
        expiry_datetime -> Timestamp,
    }
}
