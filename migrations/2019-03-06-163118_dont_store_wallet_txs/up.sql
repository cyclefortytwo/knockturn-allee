-- Your SQL goes here
ALTER TABLE orders ADD COLUMN wallet_tx_id bigint; 
ALTER TABLE orders ADD COLUMN wallet_tx_slate_id TEXT;
ALTER TABLE orders ADD COLUMN message TEXT NOT NULL DEFAULT '';
ALTER TABLE orders ADD COLUMN slate_messages TEXT[];

UPDATE orders SET wallet_tx_slate_id = (SELECT slate_id FROM txs WHERE order_id = orders.id);

