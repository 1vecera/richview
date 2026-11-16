CREATE TABLE subscribers (
    email TEXT PRIMARY KEY NOT NULL,
    consented_at TEXT NOT NULL,
    consent_version TEXT NOT NULL
) WITHOUT ROWID;
