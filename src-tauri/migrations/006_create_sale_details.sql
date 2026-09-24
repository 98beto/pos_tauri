CREATE TABLE sale_details(
id INTEGER PRIMARY KEY AUTOINCREMENT CHECK (id BETWEEN 1 AND 9007199254740991),
product_id INTEGER NOT NULL CHECK (typeof(product_id) = 'integer' AND product_id BETWEEN 1 AND 9007199254740991),
sale_id INTEGER NOT NULL CHECK (typeof(sale_id) = 'integer' AND sale_id BETWEEN 1 AND 9007199254740991),
product_name TEXT NOT NULL CHECK (length(trim(product_name, char(9)||char(10)||char(11)||char(12)||char(13)||' ')) BETWEEN 1 AND 200),
sku TEXT NOT NULL CHECK (length(trim(sku, char(9)||char(10)||char(11)||char(12)||char(13)||' ')) BETWEEN 1 AND 64),
quantity INTEGER NOT NULL CHECK (typeof(quantity) = 'integer' AND quantity BETWEEN 1 AND 9007199254740991),
unit_price INTEGER NOT NULL CHECK (typeof(unit_price) = 'integer' AND unit_price BETWEEN 1 AND 9007199254740991),

created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP CHECK (length(trim(created_at, char(9)||char(10)||char(11)||char(12)||char(13)||' ')) > 0),
updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP CHECK (length(trim(updated_at, char(9)||char(10)||char(11)||char(12)||char(13)||' ')) > 0),

FOREIGN KEY(product_id) REFERENCES products(id) ON DELETE RESTRICT,
FOREIGN KEY(sale_id) REFERENCES sales(id) ON DELETE RESTRICT,
UNIQUE(sale_id, product_id)
);
