-- This file should undo anything in `up.sql`
ALTER TABLE orders DROP COLUMN reported;
ALTER TABLE orders DROP COLUMN report_attempts;
ALTER TABLE orders DROP COLUMN last_report_attempt;
