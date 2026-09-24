CREATE TABLE categories(
id INTEGER PRIMARY KEY AUTOINCREMENT CHECK (id BETWEEN 1 AND 9007199254740991),
name TEXT COLLATE NOCASE NOT NULL UNIQUE CHECK (length(trim(name, char(9)||char(10)||char(11)||char(12)||char(13)||' ')) BETWEEN 1 AND 100),
name_search_key TEXT NOT NULL CHECK (length(name_search_key) > 0),
created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP CHECK (length(trim(created_at, char(9)||char(10)||char(11)||char(12)||char(13)||' ')) > 0),
updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP CHECK (length(trim(updated_at, char(9)||char(10)||char(11)||char(12)||char(13)||' ')) > 0)
);
