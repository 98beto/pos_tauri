CREATE TABLE sales(
id INTEGER PRIMARY KEY AUTOINCREMENT CHECK (id BETWEEN 1 AND 9007199254740991),
user_id INTEGER NOT NULL CHECK (typeof(user_id) = 'integer' AND user_id BETWEEN 1 AND 9007199254740991),
cashier_name TEXT NOT NULL CHECK (length(trim(cashier_name, char(9)||char(10)||char(11)||char(12)||char(13)||' ')) BETWEEN 1 AND 401),
cashier_email TEXT NOT NULL CHECK (
length(cashier_email) BETWEEN 3 AND 254
AND instr(cashier_email, '@') > 1
AND instr(substr(cashier_email, instr(cashier_email, '@') + 1), '@') = 0
AND instr(cashier_email, '@') < length(cashier_email)
),
checkout_token TEXT NOT NULL UNIQUE CHECK (length(trim(checkout_token, char(9)||char(10)||char(11)||char(12)||char(13)||' ')) BETWEEN 1 AND 128),
cart_fingerprint TEXT NOT NULL CHECK (length(cart_fingerprint) > 0),
sale_date TEXT NOT NULL CHECK (length(trim(sale_date, char(9)||char(10)||char(11)||char(12)||char(13)||' ')) > 0),
total INTEGER NOT NULL CHECK (typeof(total) = 'integer' AND total BETWEEN 1 AND 9007199254740991),
payment_method TEXT NOT NULL CHECK(
payment_method IN ('efectivo', 'tarjeta', 'transferencia')
),
cash_received INTEGER CHECK (cash_received IS NULL OR (typeof(cash_received) = 'integer' AND cash_received BETWEEN 0 AND 9007199254740991)),
change_amount INTEGER CHECK (change_amount IS NULL OR (typeof(change_amount) = 'integer' AND change_amount BETWEEN 0 AND 9007199254740991)),
created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP CHECK (length(trim(created_at, char(9)||char(10)||char(11)||char(12)||char(13)||' ')) > 0),
updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP CHECK (length(trim(updated_at, char(9)||char(10)||char(11)||char(12)||char(13)||' ')) > 0),
CHECK (
(payment_method = 'efectivo'
 AND cash_received IS NOT NULL
 AND cash_received >= total
 AND change_amount = cash_received - total)
OR
(payment_method <> 'efectivo'
 AND cash_received IS NULL
 AND change_amount IS NULL)
),
FOREIGN KEY(user_id) REFERENCES users(id) ON DELETE RESTRICT
);
