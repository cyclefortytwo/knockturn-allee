-- Your SQL goes here
ALTER TABLE merchants ADD COLUMN token_2fa VARCHAR(16);
ALTER TABLE merchants ADD COLUMN confirmed_2fa BOOLEAN NOT NULL DEFAULT false;
