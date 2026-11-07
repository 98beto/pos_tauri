CREATE TABLE cash_sessions (
 id INTEGER PRIMARY KEY,
 status TEXT NOT NULL CHECK(status IN ('open','closed')),
 historical INTEGER NOT NULL DEFAULT 0 CHECK(historical IN (0,1)),
 opening_amount INTEGER CHECK(opening_amount IS NULL OR (typeof(opening_amount)='integer' AND opening_amount BETWEEN 0 AND 9007199254740991)),
 opened_by INTEGER REFERENCES users(id),
 opening_operator TEXT,
 opened_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
 closed_by INTEGER REFERENCES users(id),
 closing_operator TEXT,
 closed_at TEXT,
 expected_amount INTEGER CHECK(expected_amount IS NULL OR (typeof(expected_amount)='integer' AND expected_amount BETWEEN 0 AND 9007199254740991)),
 counted_amount INTEGER CHECK(counted_amount IS NULL OR (typeof(counted_amount)='integer' AND counted_amount BETWEEN 0 AND 9007199254740991)),
 difference INTEGER CHECK(difference IS NULL OR (typeof(difference)='integer' AND difference BETWEEN -9007199254740991 AND 9007199254740991)),
 closing_breakdown TEXT,
 CHECK((historical=1 AND status='closed' AND opening_amount IS NULL AND counted_amount IS NULL AND difference IS NULL AND expected_amount IS NULL AND closed_at IS NOT NULL
 AND opened_by IS NULL AND closed_by IS NULL AND opening_operator IS NULL AND closing_operator IS NULL AND closing_breakdown IS NULL)
 OR (historical=0 AND opening_amount IS NOT NULL AND opened_by IS NOT NULL AND opening_operator IS NOT NULL AND
 ((status='open' AND closed_at IS NULL AND closed_by IS NULL AND closing_operator IS NULL AND expected_amount IS NULL AND counted_amount IS NULL AND difference IS NULL AND closing_breakdown IS NULL)
 OR (status='closed' AND closed_at IS NOT NULL AND closed_by IS NOT NULL AND closing_operator IS NOT NULL AND expected_amount IS NOT NULL AND counted_amount IS NOT NULL AND difference IS NOT NULL AND difference=counted_amount-expected_amount AND closing_breakdown IS NOT NULL))))
);
CREATE UNIQUE INDEX idx_cash_single_open ON cash_sessions(status) WHERE status='open';
INSERT INTO cash_sessions(id,status,historical,opened_at,closed_at)
 SELECT 1,'closed',1,MIN(sale_date),MAX(sale_date) FROM sales HAVING COUNT(*)>0;
ALTER TABLE sales ADD COLUMN cash_session_id INTEGER REFERENCES cash_sessions(id);
UPDATE sales SET cash_session_id=1;
CREATE INDEX idx_sales_cash_session ON sales(cash_session_id);
CREATE TABLE cash_operations (
 id INTEGER PRIMARY KEY,
 session_id INTEGER NOT NULL REFERENCES cash_sessions(id),
 kind TEXT NOT NULL CHECK(kind IN ('open','income','withdraw','close')),
 amount INTEGER NOT NULL CHECK(typeof(amount)='integer' AND amount BETWEEN 0 AND 9007199254740991 AND (kind IN ('open','close') OR amount>0)),
 reason TEXT NOT NULL CHECK(length(reason)<=500 AND (kind IN ('open','close') OR length(trim(reason))>0)),
 user_id INTEGER NOT NULL REFERENCES users(id),
 operator TEXT NOT NULL,
 operation_token TEXT NOT NULL UNIQUE CHECK(length(operation_token) BETWEEN 1 AND 128),
 intent_fingerprint TEXT NOT NULL,
 created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE INDEX idx_cash_operations_session ON cash_operations(session_id,id);
CREATE TRIGGER cash_closed_immutable BEFORE UPDATE ON cash_sessions WHEN OLD.status='closed'
 BEGIN SELECT RAISE(ABORT,'CASH_SESSION_CLOSED'); END;
CREATE TRIGGER cash_no_delete BEFORE DELETE ON cash_sessions
 BEGIN SELECT RAISE(ABORT,'CASH_SESSION_IMMUTABLE'); END;
CREATE TRIGGER cash_operation_open BEFORE INSERT ON cash_operations
 WHEN NOT EXISTS(SELECT 1 FROM cash_sessions WHERE id=NEW.session_id AND status='open')
 BEGIN SELECT RAISE(ABORT,'CASH_SESSION_CLOSED'); END;
CREATE TRIGGER cash_operation_no_update BEFORE UPDATE ON cash_operations
 BEGIN SELECT RAISE(ABORT,'CASH_OPERATION_IMMUTABLE'); END;
CREATE TRIGGER cash_operation_no_delete BEFORE DELETE ON cash_operations
 BEGIN SELECT RAISE(ABORT,'CASH_OPERATION_IMMUTABLE'); END;
CREATE TRIGGER sale_cash_open BEFORE INSERT ON sales
 WHEN NEW.cash_session_id IS NULL OR NOT EXISTS(SELECT 1 FROM cash_sessions WHERE id=NEW.cash_session_id AND status='open')
 BEGIN SELECT RAISE(ABORT,'CASH_SESSION_REQUIRED'); END;
CREATE TRIGGER sale_cash_immutable BEFORE UPDATE ON sales
 WHEN NEW.cash_session_id IS NOT OLD.cash_session_id OR EXISTS(SELECT 1 FROM cash_sessions WHERE id=OLD.cash_session_id AND status='closed')
 BEGIN SELECT RAISE(ABORT,'CASH_SESSION_IMMUTABLE'); END;
CREATE TRIGGER sale_cash_no_delete BEFORE DELETE ON sales
 WHEN EXISTS(SELECT 1 FROM cash_sessions WHERE id=OLD.cash_session_id AND status='closed')
 BEGIN SELECT RAISE(ABORT,'CASH_SESSION_IMMUTABLE'); END;
CREATE TRIGGER sale_detail_cash_no_insert BEFORE INSERT ON sale_details
 WHEN EXISTS(SELECT 1 FROM sales s JOIN cash_sessions c ON c.id=s.cash_session_id WHERE s.id=NEW.sale_id AND c.status='closed')
 BEGIN SELECT RAISE(ABORT,'CASH_SESSION_IMMUTABLE'); END;
CREATE TRIGGER sale_detail_cash_no_update BEFORE UPDATE ON sale_details
 WHEN EXISTS(SELECT 1 FROM sales s JOIN cash_sessions c ON c.id=s.cash_session_id WHERE s.id IN (OLD.sale_id,NEW.sale_id) AND c.status='closed')
 BEGIN SELECT RAISE(ABORT,'CASH_SESSION_IMMUTABLE'); END;
CREATE TRIGGER sale_detail_cash_no_delete BEFORE DELETE ON sale_details
 WHEN EXISTS(SELECT 1 FROM sales s JOIN cash_sessions c ON c.id=s.cash_session_id WHERE s.id=OLD.sale_id AND c.status='closed')
 BEGIN SELECT RAISE(ABORT,'CASH_SESSION_IMMUTABLE'); END;
