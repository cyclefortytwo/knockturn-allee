-- This file should undo anything in `up.sql`

ALTER TABLE merchants DROP COLUMN token;
ALTER TABLE merchants DROP COLUMN callback_url ;
