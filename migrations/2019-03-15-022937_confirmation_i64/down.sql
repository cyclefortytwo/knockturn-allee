-- This file should undo anything in `up.sql`
ALTER TABLE transactions ALTER COLUMN confirmations TYPE INTEGER;
