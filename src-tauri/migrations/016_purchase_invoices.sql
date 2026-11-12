CREATE TABLE purchase_invoices (
 id INTEGER PRIMARY KEY AUTOINCREMENT CHECK(id BETWEEN 1 AND 9007199254740991),
 uuid TEXT NOT NULL UNIQUE CHECK(length(uuid)=36 AND uuid=upper(uuid)),
 xml_sha256 TEXT NOT NULL CHECK(length(xml_sha256)=64 AND xml_sha256 NOT GLOB '*[^0-9a-f]*'),
 issuer_rfc TEXT NOT NULL, issuer_name TEXT NOT NULL, issuer_search_key TEXT NOT NULL,
 receiver_rfc TEXT NOT NULL, receiver_name TEXT NOT NULL,
 series TEXT, folio TEXT, number_search_key TEXT NOT NULL,
 issued_at TEXT NOT NULL, issued_date TEXT NOT NULL,
 currency TEXT NOT NULL, total TEXT NOT NULL,
 header_json TEXT NOT NULL CHECK(json_valid(header_json)),
 registered_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ','now')),
 registered_by_user_id INTEGER NOT NULL REFERENCES users(id),
 registered_by_name TEXT NOT NULL
);
CREATE INDEX purchase_invoices_emission ON purchase_invoices(issued_date DESC, issued_at DESC, id DESC);
CREATE INDEX purchase_invoices_supplier ON purchase_invoices(issuer_rfc, issued_at DESC, id DESC);
CREATE TABLE purchase_invoice_lines (
 invoice_id INTEGER NOT NULL REFERENCES purchase_invoices(id),
 position INTEGER NOT NULL CHECK(position BETWEEN 0 AND 9999),
 concept_json TEXT NOT NULL CHECK(json_valid(concept_json)),
 PRIMARY KEY(invoice_id,position)
);
CREATE TABLE purchase_invoice_attachments (
 invoice_id INTEGER NOT NULL REFERENCES purchase_invoices(id),
 kind TEXT NOT NULL CHECK(kind IN ('xml','pdf')),
 original_name TEXT NOT NULL CHECK(length(original_name) BETWEEN 1 AND 200),
 mime_type TEXT NOT NULL,
 size_bytes INTEGER NOT NULL,
 sha256 TEXT NOT NULL CHECK(length(sha256)=64 AND sha256 NOT GLOB '*[^0-9a-f]*'),
 content BLOB NOT NULL CHECK(typeof(content)='blob' AND length(content)=size_bytes),
 PRIMARY KEY(invoice_id,kind),
 CHECK((kind='xml' AND mime_type='application/xml' AND size_bytes BETWEEN 1 AND 5242880)
 OR (kind='pdf' AND mime_type='application/pdf' AND size_bytes BETWEEN 5 AND 15728640 AND substr(content,1,5)=x'255044462d'))
);
