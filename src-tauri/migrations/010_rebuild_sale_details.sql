ALTER TABLE sale_details RENAME TO sale_details_v8;

CREATE TABLE sale_details(
    id INTEGER PRIMARY KEY AUTOINCREMENT
        CHECK (id BETWEEN 1 AND 9007199254740991),
    product_id INTEGER NOT NULL
        CHECK (typeof(product_id) = 'integer' AND product_id BETWEEN 1 AND 9007199254740991),
    sale_id INTEGER NOT NULL
        CHECK (typeof(sale_id) = 'integer' AND sale_id BETWEEN 1 AND 9007199254740991),
    record_version INTEGER NOT NULL DEFAULT 1
        CHECK (typeof(record_version) = 'integer' AND record_version IN (1, 2)),
    presentation_id INTEGER
        CHECK (presentation_id IS NULL OR (typeof(presentation_id) = 'integer' AND presentation_id BETWEEN 1 AND 9007199254740991)),
    product_name TEXT NOT NULL
        CHECK (length(trim(product_name, char(9)||char(10)||char(11)||char(12)||char(13)||' ')) BETWEEN 1 AND 200),
    sku TEXT NOT NULL
        CHECK (length(trim(sku, char(9)||char(10)||char(11)||char(12)||char(13)||' ')) BETWEEN 1 AND 64),
    presentation_name TEXT
        CHECK (presentation_name IS NULL OR length(trim(presentation_name, char(9)||char(10)||char(11)||char(12)||char(13)||' ')) BETWEEN 1 AND 100),
    presentation_barcode TEXT
        CHECK (presentation_barcode IS NULL OR length(trim(presentation_barcode, char(9)||char(10)||char(11)||char(12)||char(13)||' ')) BETWEEN 1 AND 128),
    base_unit TEXT CHECK (base_unit IS NULL OR base_unit IN ('piece', 'gram', 'milliliter')),
    presentation_mode TEXT
        CHECK (presentation_mode IS NULL OR presentation_mode IN ('fixed', 'measured')),
    presentation_base_quantity INTEGER
        CHECK (presentation_base_quantity IS NULL OR (typeof(presentation_base_quantity) = 'integer' AND presentation_base_quantity BETWEEN 1 AND 9007199254740991)),
    quantity INTEGER NOT NULL
        CHECK (typeof(quantity) = 'integer' AND quantity BETWEEN 1 AND 9007199254740991),
    unit_price INTEGER NOT NULL
        CHECK (typeof(unit_price) = 'integer' AND unit_price BETWEEN 1 AND 9007199254740991),
    entered_quantity INTEGER
        CHECK (entered_quantity IS NULL OR (typeof(entered_quantity) = 'integer' AND entered_quantity BETWEEN 1 AND 9007199254740991)),
    base_quantity INTEGER
        CHECK (base_quantity IS NULL OR (typeof(base_quantity) = 'integer' AND base_quantity BETWEEN 1 AND 9007199254740991)),
    line_total INTEGER
        CHECK (line_total IS NULL OR (typeof(line_total) = 'integer' AND line_total BETWEEN 1 AND 9007199254740991)),
    movement_cost_cents INTEGER
        CHECK (movement_cost_cents IS NULL OR (typeof(movement_cost_cents) = 'integer' AND movement_cost_cents BETWEEN 0 AND 9007199254740991)),
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
        CHECK (length(trim(created_at, char(9)||char(10)||char(11)||char(12)||char(13)||' ')) > 0),
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
        CHECK (length(trim(updated_at, char(9)||char(10)||char(11)||char(12)||char(13)||' ')) > 0),
    CHECK (
        record_version = 1
        OR (presentation_id IS NOT NULL
            AND presentation_name IS NOT NULL
            AND base_unit IS NOT NULL
            AND presentation_mode IS NOT NULL
            AND presentation_base_quantity IS NOT NULL
            AND entered_quantity IS NOT NULL
            AND base_quantity IS NOT NULL
            AND line_total IS NOT NULL)
    ),
    FOREIGN KEY(product_id) REFERENCES products(id) ON DELETE RESTRICT,
    FOREIGN KEY(sale_id) REFERENCES sales(id) ON DELETE RESTRICT,
    FOREIGN KEY(presentation_id, product_id)
        REFERENCES product_presentations(id, product_id) ON DELETE RESTRICT
);

INSERT INTO sale_details (
    id, product_id, sale_id, record_version, presentation_id,
    product_name, sku, presentation_name, presentation_barcode,
    base_unit, presentation_mode, presentation_base_quantity,
    quantity, unit_price, entered_quantity, base_quantity, line_total,
    movement_cost_cents, created_at, updated_at
)
SELECT
    old.id, old.product_id, old.sale_id, 1, presentation.id,
    old.product_name, old.sku, presentation.name, presentation.barcode,
    product.base_unit, presentation.mode, presentation.base_quantity,
    old.quantity, old.unit_price, old.quantity, old.quantity,
    CASE
        WHEN old.quantity <= 9007199254740991 / old.unit_price
        THEN old.quantity * old.unit_price
        ELSE NULL
    END,
    NULL, old.created_at, old.updated_at
FROM sale_details_v8 AS old
LEFT JOIN products AS product ON product.id = old.product_id
LEFT JOIN product_presentations AS presentation
  ON presentation.product_id = old.product_id AND presentation.is_default = 1;

UPDATE sqlite_sequence
SET seq = (
    SELECT max(seq)
    FROM (
        SELECT seq FROM sqlite_sequence WHERE name = 'sale_details'
        UNION ALL
        SELECT seq FROM sqlite_sequence WHERE name = 'sale_details_v8'
        UNION ALL
        SELECT coalesce(max(id), 0) FROM sale_details
    )
)
WHERE name = 'sale_details';

INSERT INTO sqlite_sequence (name, seq)
SELECT 'sale_details', max(seq)
FROM (
    SELECT seq FROM sqlite_sequence WHERE name = 'sale_details_v8'
    UNION ALL
    SELECT coalesce(max(id), 0) FROM sale_details
)
WHERE NOT EXISTS (
    SELECT 1 FROM sqlite_sequence WHERE name = 'sale_details'
);

DROP TABLE sale_details_v8;

CREATE INDEX idx_sale_details_product_id ON sale_details (product_id);
CREATE INDEX idx_sale_details_sale_id ON sale_details (sale_id);
CREATE INDEX idx_sale_details_presentation_id ON sale_details (presentation_id);
CREATE UNIQUE INDEX idx_sale_details_sale_presentation_v2
ON sale_details (sale_id, presentation_id)
WHERE record_version = 2;
CREATE UNIQUE INDEX idx_sale_details_line_identity
ON sale_details (id, sale_id, product_id, presentation_id);
