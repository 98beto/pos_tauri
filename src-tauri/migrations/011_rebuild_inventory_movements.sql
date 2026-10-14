ALTER TABLE inventory_movements RENAME TO inventory_movements_v8;

CREATE TABLE inventory_movements(
    id INTEGER PRIMARY KEY AUTOINCREMENT
        CHECK (id BETWEEN 1 AND 9007199254740991),
    product_id INTEGER NOT NULL
        CHECK (typeof(product_id) = 'integer' AND product_id BETWEEN 1 AND 9007199254740991),
    product_name TEXT NOT NULL
        CHECK (length(trim(product_name, char(9)||char(10)||char(11)||char(12)||char(13)||' ')) BETWEEN 1 AND 200),
    sku TEXT NOT NULL
        CHECK (length(trim(sku, char(9)||char(10)||char(11)||char(12)||char(13)||' ')) BETWEEN 1 AND 64),
    product_search_key TEXT NOT NULL CHECK (length(product_search_key) > 0),
    sku_search_key TEXT NOT NULL CHECK (length(sku_search_key) > 0),
    sale_id INTEGER
        CHECK (sale_id IS NULL OR (typeof(sale_id) = 'integer' AND sale_id BETWEEN 1 AND 9007199254740991)),
    record_version INTEGER NOT NULL DEFAULT 1
        CHECK (typeof(record_version) = 'integer' AND record_version IN (1, 2)),
    presentation_id INTEGER
        CHECK (presentation_id IS NULL OR (typeof(presentation_id) = 'integer' AND presentation_id BETWEEN 1 AND 9007199254740991)),
    sale_detail_id INTEGER
        CHECK (sale_detail_id IS NULL OR (typeof(sale_detail_id) = 'integer' AND sale_detail_id BETWEEN 1 AND 9007199254740991)),
    presentation_name TEXT
        CHECK (presentation_name IS NULL OR length(trim(presentation_name, char(9)||char(10)||char(11)||char(12)||char(13)||' ')) BETWEEN 1 AND 100),
    presentation_barcode TEXT
        CHECK (presentation_barcode IS NULL OR length(trim(presentation_barcode, char(9)||char(10)||char(11)||char(12)||char(13)||' ')) BETWEEN 1 AND 128),
    base_unit TEXT CHECK (base_unit IS NULL OR base_unit IN ('piece', 'gram', 'milliliter')),
    presentation_mode TEXT
        CHECK (presentation_mode IS NULL OR presentation_mode IN ('fixed', 'measured')),
    presentation_base_quantity INTEGER
        CHECK (presentation_base_quantity IS NULL OR (typeof(presentation_base_quantity) = 'integer' AND presentation_base_quantity BETWEEN 1 AND 9007199254740991)),
    unit_price_cents INTEGER
        CHECK (unit_price_cents IS NULL OR (typeof(unit_price_cents) = 'integer' AND unit_price_cents BETWEEN 0 AND 9007199254740991)),
    origin TEXT NOT NULL CHECK (origin IN ('sale', 'initial_stock', 'manual')),
    reason TEXT NOT NULL CHECK (reason IN ('venta', 'compra', 'ajuste', 'valoracion')),
    type TEXT NOT NULL CHECK (type IN ('entrada', 'salida', 'valoracion')),
    quantity INTEGER NOT NULL
        CHECK (typeof(quantity) = 'integer' AND quantity BETWEEN 0 AND 9007199254740991),
    entered_quantity INTEGER
        CHECK (entered_quantity IS NULL OR (typeof(entered_quantity) = 'integer' AND entered_quantity BETWEEN 0 AND 9007199254740991)),
    base_quantity INTEGER
        CHECK (base_quantity IS NULL OR (typeof(base_quantity) = 'integer' AND base_quantity BETWEEN 0 AND 9007199254740991)),
    stock_before INTEGER
        CHECK (stock_before IS NULL OR (typeof(stock_before) = 'integer' AND stock_before BETWEEN 0 AND 9007199254740991)),
    stock_after INTEGER
        CHECK (stock_after IS NULL OR (typeof(stock_after) = 'integer' AND stock_after BETWEEN 0 AND 9007199254740991)),
    movement_cost_cents INTEGER
        CHECK (movement_cost_cents IS NULL OR (typeof(movement_cost_cents) = 'integer' AND movement_cost_cents BETWEEN 0 AND 9007199254740991)),
    inventory_cost_before_cents INTEGER
        CHECK (inventory_cost_before_cents IS NULL OR (typeof(inventory_cost_before_cents) = 'integer' AND inventory_cost_before_cents BETWEEN 0 AND 9007199254740991)),
    inventory_cost_after_cents INTEGER
        CHECK (inventory_cost_after_cents IS NULL OR (typeof(inventory_cost_after_cents) = 'integer' AND inventory_cost_after_cents BETWEEN 0 AND 9007199254740991)),
    supplier TEXT
        CHECK (supplier IS NULL OR length(trim(supplier, char(9)||char(10)||char(11)||char(12)||char(13)||' ')) BETWEEN 1 AND 200),
    provider_search_key TEXT
        CHECK (provider_search_key IS NULL OR length(provider_search_key) > 0),
    document_reference TEXT
        CHECK (document_reference IS NULL OR length(trim(document_reference, char(9)||char(10)||char(11)||char(12)||char(13)||' ')) BETWEEN 1 AND 200),
    document_reference_search_key TEXT
        CHECK (document_reference_search_key IS NULL OR length(document_reference_search_key) > 0),
    note TEXT
        CHECK (note IS NULL OR length(trim(note, char(9)||char(10)||char(11)||char(12)||char(13)||' ')) BETWEEN 1 AND 1000),
    note_search_key TEXT
        CHECK (note_search_key IS NULL OR length(note_search_key) > 0),
    operation_token TEXT
        CHECK (operation_token IS NULL OR length(operation_token) BETWEEN 1 AND 128),
    intent_fingerprint TEXT
        CHECK (intent_fingerprint IS NULL OR length(intent_fingerprint) BETWEEN 1 AND 2048),
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
        CHECK (length(trim(created_at, char(9)||char(10)||char(11)||char(12)||char(13)||' ')) > 0),
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
        CHECK (length(trim(updated_at, char(9)||char(10)||char(11)||char(12)||char(13)||' ')) > 0),
    CHECK ((supplier IS NULL) = (provider_search_key IS NULL)),
    CHECK ((document_reference IS NULL) = (document_reference_search_key IS NULL)),
    CHECK ((note IS NULL) = (note_search_key IS NULL)),
    CHECK (
        (record_version = 1 AND (
            (origin = 'sale' AND reason = 'venta' AND type = 'salida' AND sale_id IS NOT NULL
             AND operation_token IS NULL AND intent_fingerprint IS NULL AND quantity > 0)
            OR (origin = 'initial_stock' AND reason = 'ajuste' AND type = 'entrada' AND sale_id IS NULL
                AND operation_token IS NULL AND intent_fingerprint IS NULL AND quantity > 0)
            OR (origin = 'manual' AND sale_id IS NULL AND operation_token IS NOT NULL
                AND intent_fingerprint IS NOT NULL AND quantity > 0
                AND ((reason = 'compra' AND type = 'entrada')
                     OR (reason = 'ajuste' AND type IN ('entrada', 'salida'))))
        ))
        OR (record_version = 2
            AND stock_before IS NOT NULL AND stock_after IS NOT NULL
            AND entered_quantity IS NOT NULL AND base_quantity IS NOT NULL
            AND ((type IN ('entrada', 'salida') AND quantity = base_quantity)
                 OR (type = 'valoracion' AND quantity = 0
                     AND entered_quantity = 0 AND base_quantity = 0))
            AND (
                (origin = 'sale' AND reason = 'venta' AND type = 'salida'
                 AND sale_id IS NOT NULL AND sale_detail_id IS NOT NULL AND presentation_id IS NOT NULL
                 AND presentation_name IS NOT NULL AND base_unit IS NOT NULL
                 AND presentation_mode IS NOT NULL AND presentation_base_quantity IS NOT NULL
                 AND unit_price_cents IS NOT NULL AND unit_price_cents > 0 AND base_quantity > 0
                 AND operation_token IS NULL AND intent_fingerprint IS NULL)
                OR (origin = 'initial_stock' AND reason = 'ajuste' AND type = 'entrada'
                     AND sale_id IS NULL AND sale_detail_id IS NULL AND base_quantity > 0
                     AND ((unit_price_cents IS NULL AND movement_cost_cents IS NULL)
                          OR (unit_price_cents IS NOT NULL AND unit_price_cents > 0
                              AND movement_cost_cents IS NOT NULL AND movement_cost_cents > 0))
                     AND operation_token IS NULL AND intent_fingerprint IS NULL)
                OR (origin = 'manual' AND sale_id IS NULL AND sale_detail_id IS NULL
                    AND operation_token IS NOT NULL AND intent_fingerprint IS NOT NULL
                    AND ((reason = 'compra' AND type = 'entrada' AND base_quantity > 0
                          AND unit_price_cents IS NOT NULL AND unit_price_cents > 0
                          AND movement_cost_cents IS NOT NULL AND movement_cost_cents > 0)
                         OR (reason = 'ajuste' AND type = 'entrada' AND base_quantity > 0
                              AND unit_price_cents IS NOT NULL AND unit_price_cents > 0
                              AND movement_cost_cents IS NOT NULL AND movement_cost_cents > 0)
                         OR (reason = 'ajuste' AND type = 'salida' AND base_quantity > 0
                             AND ((movement_cost_cents IS NULL AND unit_price_cents IS NULL)
                                  OR (movement_cost_cents IS NOT NULL AND unit_price_cents IS NOT NULL)))
                         OR (reason = 'valoracion' AND type = 'valoracion'
                              AND entered_quantity = 0 AND base_quantity = 0
                              AND unit_price_cents IS NULL AND stock_before = stock_after
                              AND movement_cost_cents IS NOT NULL)))
            )
            AND (
                (type = 'entrada'
                 AND base_quantity > 0
                 AND stock_after = stock_before + base_quantity
                 AND ((inventory_cost_before_cents IS NULL AND inventory_cost_after_cents IS NULL)
                      OR (inventory_cost_before_cents IS NOT NULL
                          AND movement_cost_cents IS NOT NULL
                          AND inventory_cost_after_cents = inventory_cost_before_cents + movement_cost_cents)))
                OR (type = 'salida'
                    AND base_quantity > 0
                    AND stock_before = stock_after + base_quantity
                    AND ((inventory_cost_before_cents IS NULL
                          AND movement_cost_cents IS NULL
                          AND ((stock_after > 0 AND inventory_cost_after_cents IS NULL)
                               OR (stock_after = 0 AND inventory_cost_after_cents = 0)))
                         OR (inventory_cost_before_cents IS NOT NULL
                             AND movement_cost_cents IS NOT NULL
                             AND inventory_cost_after_cents = inventory_cost_before_cents - movement_cost_cents)))
                OR (type = 'valoracion'
                    AND entered_quantity = 0 AND base_quantity = 0
                    AND stock_before = stock_after
                    AND unit_price_cents IS NULL
                    AND movement_cost_cents = inventory_cost_after_cents
                    AND (movement_cost_cents = 0 OR stock_after > 0))
            ))
    ),
    FOREIGN KEY(product_id) REFERENCES products(id) ON DELETE RESTRICT,
    FOREIGN KEY(sale_id) REFERENCES sales(id) ON DELETE RESTRICT,
    FOREIGN KEY(presentation_id, product_id)
        REFERENCES product_presentations(id, product_id) ON DELETE RESTRICT,
    FOREIGN KEY(sale_detail_id, sale_id, product_id, presentation_id)
        REFERENCES sale_details(id, sale_id, product_id, presentation_id) ON DELETE RESTRICT
);

INSERT INTO inventory_movements (
    id, product_id, product_name, sku, product_search_key, sku_search_key,
    sale_id, record_version, presentation_id, sale_detail_id,
    presentation_name, presentation_barcode, base_unit, presentation_mode,
    presentation_base_quantity, unit_price_cents, origin, reason, type, quantity,
    entered_quantity, base_quantity, stock_before, stock_after,
    movement_cost_cents, inventory_cost_before_cents, inventory_cost_after_cents,
    supplier, provider_search_key, document_reference, document_reference_search_key,
    note, note_search_key, operation_token, intent_fingerprint,
    created_at, updated_at
)
SELECT
    old.id, old.product_id, old.product_name, old.sku,
    old.product_search_key, old.sku_search_key, old.sale_id, 1,
    presentation.id,
    CASE WHEN old.origin = 'sale' THEN detail.id ELSE NULL END,
    presentation.name, presentation.barcode, product.base_unit,
    presentation.mode, presentation.base_quantity, NULL,
    old.origin, old.reason, old.type, old.quantity,
    old.quantity, old.quantity, NULL, NULL, NULL, NULL, NULL,
    NULL, NULL, NULL, NULL, NULL, NULL, old.operation_token, old.intent_fingerprint,
    old.created_at, old.updated_at
FROM inventory_movements_v8 AS old
JOIN products AS product ON product.id = old.product_id
JOIN product_presentations AS presentation
  ON presentation.product_id = old.product_id AND presentation.is_default = 1
LEFT JOIN sale_details AS detail
  ON old.origin = 'sale'
 AND detail.sale_id = old.sale_id
 AND detail.product_id = old.product_id;

UPDATE sqlite_sequence
SET seq = (
    SELECT max(seq)
    FROM (
        SELECT seq FROM sqlite_sequence WHERE name = 'inventory_movements'
        UNION ALL
        SELECT seq FROM sqlite_sequence WHERE name = 'inventory_movements_v8'
        UNION ALL
        SELECT coalesce(max(id), 0) FROM inventory_movements
    )
)
WHERE name = 'inventory_movements';

INSERT INTO sqlite_sequence (name, seq)
SELECT 'inventory_movements', max(seq)
FROM (
    SELECT seq FROM sqlite_sequence WHERE name = 'inventory_movements_v8'
    UNION ALL
    SELECT coalesce(max(id), 0) FROM inventory_movements
)
WHERE NOT EXISTS (
    SELECT 1 FROM sqlite_sequence WHERE name = 'inventory_movements'
);

DROP TABLE inventory_movements_v8;

CREATE INDEX idx_inventory_movements_product_created_at
ON inventory_movements (product_id, created_at DESC);
CREATE INDEX idx_inventory_movements_sale_id ON inventory_movements (sale_id);
CREATE INDEX idx_inventory_movements_reason_type ON inventory_movements (reason, type);
CREATE INDEX idx_inventory_movements_origin ON inventory_movements (origin);
CREATE INDEX idx_inventory_movements_product_search_key
ON inventory_movements (product_search_key);
CREATE INDEX idx_inventory_movements_sku_search_key
ON inventory_movements (sku_search_key);
CREATE UNIQUE INDEX idx_inventory_movements_operation_token
ON inventory_movements (operation_token)
WHERE operation_token IS NOT NULL;
CREATE UNIQUE INDEX idx_inventory_movements_sale_detail_id
ON inventory_movements (sale_detail_id)
WHERE sale_detail_id IS NOT NULL;
CREATE INDEX idx_inventory_movements_provider_search_key
ON inventory_movements (provider_search_key);
CREATE INDEX idx_inventory_movements_document_reference_search_key
ON inventory_movements (document_reference_search_key);
CREATE INDEX idx_inventory_movements_note_search_key
ON inventory_movements (note_search_key);
