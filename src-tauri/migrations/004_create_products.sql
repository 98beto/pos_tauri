CREATE TABLE products(
id INTEGER PRIMARY KEY AUTOINCREMENT CHECK (id BETWEEN 1 AND 9007199254740991),
name TEXT NOT NULL CHECK (length(trim(name, char(9)||char(10)||char(11)||char(12)||char(13)||' ')) BETWEEN 1 AND 200),
name_search_key TEXT NOT NULL CHECK (length(name_search_key) > 0),
price INTEGER NOT NULL CHECK (typeof(price) = 'integer' AND price BETWEEN 1 AND 9007199254740991),
stock INTEGER NOT NULL CHECK (typeof(stock) = 'integer' AND stock BETWEEN 0 AND 9007199254740991),
sku TEXT COLLATE NOCASE NOT NULL UNIQUE CHECK (
sku COLLATE BINARY = upper(sku) COLLATE BINARY
AND sku COLLATE BINARY = trim(sku, char(9)||char(10)||char(11)||char(12)||char(13)||' ') COLLATE BINARY
AND length(sku) BETWEEN 1 AND 64
),
sku_search_key TEXT NOT NULL CHECK (length(sku_search_key) > 0),
brand_id INTEGER NOT NULL CHECK (typeof(brand_id) = 'integer' AND brand_id BETWEEN 1 AND 9007199254740991),
category_id INTEGER NOT NULL CHECK (typeof(category_id) = 'integer' AND category_id BETWEEN 1 AND 9007199254740991),
created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP CHECK (length(trim(created_at, char(9)||char(10)||char(11)||char(12)||char(13)||' ')) > 0),
updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP CHECK (length(trim(updated_at, char(9)||char(10)||char(11)||char(12)||char(13)||' ')) > 0),
FOREIGN KEY(category_id) REFERENCES categories(id)
ON DELETE RESTRICT,
FOREIGN KEY(brand_id) REFERENCES brands(id)
ON DELETE RESTRICT
);
