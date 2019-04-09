CREATE TABLE txs (
  slate_id TEXT PRIMARY KEY,
  created_at TIMESTAMP NOT NULL,
  confirmed BOOLEAN NOT NULL DEFAULT 'f',
  confirmed_at TIMESTAMP,
  fee int8,
  messages TEXT[] NOT NULL,
  num_inputs int8 NOT NULL DEFAULT 0,
  num_outputs int8 NOT NULL DEFAULT 0,
  tx_type TEXT NOT NULL,
  order_id uuid NOT NULL,
  updated_at TIMESTAMP NOT NULL,
  FOREIGN KEY (order_id) REFERENCES orders (id)
)
