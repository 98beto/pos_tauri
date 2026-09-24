-- SQLite has no Unicode-aware trim; services use Rust str::trim for that layer.
CREATE TABLE users(
id INTEGER PRIMARY KEY AUTOINCREMENT CHECK (id BETWEEN 1 AND 9007199254740991),
first_name TEXT NOT NULL CHECK (length(trim(first_name, char(9)||char(10)||char(11)||char(12)||char(13)||' ')) BETWEEN 1 AND 100),
last_name TEXT NOT NULL CHECK (length(trim(last_name, char(9)||char(10)||char(11)||char(12)||char(13)||' ')) BETWEEN 1 AND 100),
email TEXT COLLATE NOCASE NOT NULL UNIQUE CHECK (
length(email) BETWEEN 3 AND 254
AND instr(email, '@') > 1
AND instr(substr(email, instr(email, '@') + 1), '@') = 0
AND instr(email, '@') < length(email)
),
password_hash TEXT NOT NULL CHECK (length(trim(password_hash, char(9)||char(10)||char(11)||char(12)||char(13)||' ')) BETWEEN 1 AND 512),
created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP CHECK (length(trim(created_at, char(9)||char(10)||char(11)||char(12)||char(13)||' ')) > 0),
updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP CHECK (length(trim(updated_at, char(9)||char(10)||char(11)||char(12)||char(13)||' ')) > 0)
);
