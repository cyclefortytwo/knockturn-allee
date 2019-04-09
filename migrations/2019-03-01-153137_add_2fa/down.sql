-- This file should undo anything in `up.sql`
ALTER TABLE merchants DROP COLUMN token_2fa;
ALTER TABLE merchants DROP COLUMN confirmed_2fa;
