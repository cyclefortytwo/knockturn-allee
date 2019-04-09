-- Your SQL goes here

ALTER TABLE orders ADD COLUMN reported BOOLEAN NOT NULL DEFAULT FALSE;
ALTER TABLE orders ADD COLUMN report_attempts int NOT NULL DEFAULT 0;
ALTER TABLE orders ADD COLUMN next_report_attempt TIMESTAMP;
