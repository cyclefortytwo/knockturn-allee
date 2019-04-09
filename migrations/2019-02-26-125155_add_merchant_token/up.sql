-- Your SQL goes here

ALTER TABLE merchants ADD COLUMN token text not null DEFAULT 'foo';
ALTER TABLE merchants ALTER COLUMN token DROP DEFAULT;
ALTER TABLE merchants ADD COLUMN callback_url text;
