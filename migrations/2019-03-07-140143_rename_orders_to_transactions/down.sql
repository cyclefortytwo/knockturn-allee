-- This file should undo anything in `up.sql`
ALTER TABLE transactions RENAME TO orders;
