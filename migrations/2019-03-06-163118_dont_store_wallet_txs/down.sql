-- This file should undo anything in `up.sql`
ALTER TABLE orders DROP COLUMN wallet_tx_id; 
ALTER TABLE orders DROP COLUMN wallet_tx_slate_id;
ALTER TABLE orders DROP COLUMN message;
ALTER TABLE orders DROP COLUMN slate_messages;



