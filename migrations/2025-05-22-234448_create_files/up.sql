-- Your SQL goes here

CREATE TABLE mochifiles (
	mmid TEXT PRIMARY KEY NOT NULL,
	name TEXT NOT NULL,
	mime_type TEXT NOT NULL,
	hash TEXT NOT NULL UNIQUE,
	upload_datetime DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
	expiry_datetime DATETIME NOT NULL
)
