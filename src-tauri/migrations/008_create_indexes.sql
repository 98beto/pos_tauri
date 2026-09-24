CREATE INDEX idx_products_category_id ON products (category_id);
CREATE INDEX idx_products_brand_id ON products (brand_id);
CREATE INDEX idx_products_name ON products (name COLLATE NOCASE);
CREATE INDEX idx_products_sku ON products (sku COLLATE NOCASE);
CREATE INDEX idx_products_name_search_key ON products (name_search_key);
CREATE UNIQUE INDEX idx_products_sku_search_key ON products (sku_search_key);

CREATE UNIQUE INDEX idx_brands_name_search_key ON brands (name_search_key);
CREATE UNIQUE INDEX idx_categories_name_search_key ON categories (name_search_key);

CREATE INDEX idx_sales_user_id ON sales (user_id);
CREATE INDEX idx_sales_sale_date ON sales (sale_date DESC);
CREATE INDEX idx_sales_payment_method_sale_date
ON sales (payment_method, sale_date DESC);

CREATE INDEX idx_sale_details_product_id ON sale_details (product_id);
CREATE INDEX idx_sale_details_sale_id ON sale_details (sale_id);

CREATE INDEX idx_inventory_movements_product_created_at
ON inventory_movements (product_id, created_at DESC);

CREATE INDEX idx_inventory_movements_sale_id
ON inventory_movements (sale_id);

CREATE INDEX idx_inventory_movements_reason_type
ON inventory_movements (reason, type);

CREATE INDEX idx_inventory_movements_origin
ON inventory_movements (origin);

CREATE INDEX idx_inventory_movements_product_search_key
ON inventory_movements (product_search_key);

CREATE INDEX idx_inventory_movements_sku_search_key
ON inventory_movements (sku_search_key);

CREATE UNIQUE INDEX idx_inventory_movements_operation_token
ON inventory_movements (operation_token)
WHERE operation_token IS NOT NULL;
