CREATE TABLE orders (
  id UUID PRIMARY KEY,
  external_id TEXT NOT NULL,
  merchant_id TEXT NOT NULL,
  grin_amount BIGINT NOT NULL,
  amount JSONB NOT NULL,
  status SMALLINT NOT NULL,
  confirmations INTEGER NOT NULL,
  email TEXT,
  created_at TIMESTAMP NOT NULL,
  updated_at TIMESTAMP NOT NULL,
  UNIQUE (merchant_id, external_id),
  FOREIGN KEY (merchant_id) REFERENCES merchants (id)
);
