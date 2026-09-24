CREATE TABLE inventory_movements(
id INTEGER PRIMARY KEY AUTOINCREMENT CHECK (id BETWEEN 1 AND 9007199254740991),
product_id INTEGER NOT NULL CHECK (typeof(product_id) = 'integer' AND product_id BETWEEN 1 AND 9007199254740991),
product_name TEXT NOT NULL CHECK (length(trim(product_name, char(9)||char(10)||char(11)||char(12)||char(13)||' ')) BETWEEN 1 AND 200),
sku TEXT NOT NULL CHECK (length(trim(sku, char(9)||char(10)||char(11)||char(12)||char(13)||' ')) BETWEEN 1 AND 64),
product_search_key TEXT NOT NULL CHECK (length(product_search_key) > 0),
sku_search_key TEXT NOT NULL CHECK (length(sku_search_key) > 0),
sale_id INTEGER CHECK (sale_id IS NULL OR (typeof(sale_id) = 'integer' AND sale_id BETWEEN 1 AND 9007199254740991)),
origin TEXT NOT NULL CHECK (origin IN ('sale', 'initial_stock', 'manual')),
reason TEXT NOT NULL CHECK(
reason IN ('venta', 'compra', 'ajuste')
),
type TEXT NOT NULL CHECK (
type IN ('entrada','salida')
),
quantity INTEGER NOT NULL CHECK (typeof(quantity) = 'integer' AND quantity BETWEEN 1 AND 9007199254740991),
operation_token TEXT CHECK (operation_token IS NULL OR length(operation_token) BETWEEN 1 AND 128),
intent_fingerprint TEXT CHECK (intent_fingerprint IS NULL OR length(intent_fingerprint) BETWEEN 1 AND 512),
created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP CHECK (length(trim(created_at, char(9)||char(10)||char(11)||char(12)||char(13)||' ')) > 0),
updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP CHECK (length(trim(updated_at, char(9)||char(10)||char(11)||char(12)||char(13)||' ')) > 0),

CHECK (
(origin = 'sale'
 AND reason = 'venta' AND type = 'salida' AND sale_id IS NOT NULL
 AND operation_token IS NULL AND intent_fingerprint IS NULL)
OR (origin = 'initial_stock'
    AND reason = 'ajuste' AND type = 'entrada' AND sale_id IS NULL
    AND operation_token IS NULL AND intent_fingerprint IS NULL)
OR (origin = 'manual'
    AND sale_id IS NULL AND operation_token IS NOT NULL AND intent_fingerprint IS NOT NULL
    AND ((reason = 'compra' AND type = 'entrada')
         OR (reason = 'ajuste' AND type IN ('entrada', 'salida'))))
),
FOREIGN KEY(product_id) REFERENCES products(id) ON DELETE RESTRICT,
FOREIGN KEY(sale_id) REFERENCES sales(id) ON DELETE RESTRICT,
UNIQUE(sale_id, product_id)
);
