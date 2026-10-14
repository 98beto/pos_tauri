ALTER TABLE products
ADD COLUMN base_unit TEXT NOT NULL DEFAULT 'piece'
CHECK (base_unit IN ('piece', 'gram', 'milliliter'));

ALTER TABLE products
ADD COLUMN inventory_cost_cents INTEGER
CHECK (
    inventory_cost_cents IS NULL
    OR (typeof(inventory_cost_cents) = 'integer'
        AND inventory_cost_cents BETWEEN 0 AND 9007199254740991)
);

UPDATE products
SET inventory_cost_cents = 0
WHERE stock = 0;

CREATE TABLE product_presentations(
    id INTEGER PRIMARY KEY AUTOINCREMENT
        CHECK (id BETWEEN 1 AND 9007199254740991),
    product_id INTEGER NOT NULL
        CHECK (typeof(product_id) = 'integer' AND product_id BETWEEN 1 AND 9007199254740991),
    name TEXT NOT NULL
        CHECK (length(trim(name, char(9)||char(10)||char(11)||char(12)||char(13)||' ')) BETWEEN 1 AND 100),
    mode TEXT NOT NULL CHECK (mode IN ('fixed', 'measured')),
    base_quantity INTEGER NOT NULL
        CHECK (typeof(base_quantity) = 'integer' AND base_quantity BETWEEN 1 AND 9007199254740991),
    price INTEGER NOT NULL
        CHECK (typeof(price) = 'integer' AND price BETWEEN 1 AND 9007199254740991),
    barcode TEXT CHECK (
        barcode IS NULL
        OR length(trim(barcode, char(9)||char(10)||char(11)||char(12)||char(13)||' ')) BETWEEN 1 AND 128
    ),
    barcode_search_key TEXT CHECK (
        barcode_search_key IS NULL OR length(barcode_search_key) BETWEEN 1 AND 128
    ),
    is_default INTEGER NOT NULL DEFAULT 0
        CHECK (typeof(is_default) = 'integer' AND is_default IN (0, 1)),
    is_active INTEGER NOT NULL DEFAULT 1
        CHECK (typeof(is_active) = 'integer' AND is_active IN (0, 1)),
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
        CHECK (length(trim(created_at, char(9)||char(10)||char(11)||char(12)||char(13)||' ')) > 0),
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
        CHECK (length(trim(updated_at, char(9)||char(10)||char(11)||char(12)||char(13)||' ')) > 0),
    CHECK ((barcode IS NULL) = (barcode_search_key IS NULL)),
    CHECK (is_default = 0 OR is_active = 1),
    FOREIGN KEY(product_id) REFERENCES products(id) ON DELETE RESTRICT
);

CREATE UNIQUE INDEX idx_product_presentations_barcode_search_key
ON product_presentations (barcode_search_key)
WHERE barcode_search_key IS NOT NULL;

CREATE UNIQUE INDEX idx_product_presentations_one_default
ON product_presentations (product_id)
WHERE is_default = 1;

CREATE INDEX idx_product_presentations_product_active
ON product_presentations (product_id, is_active);

CREATE UNIQUE INDEX idx_product_presentations_identity
ON product_presentations (id, product_id);

INSERT INTO product_presentations (
    product_id, name, mode, base_quantity, price, is_default, is_active,
    created_at, updated_at
)
SELECT id, 'Pieza', 'fixed', 1, price, 1, 1, created_at, updated_at
FROM products;
