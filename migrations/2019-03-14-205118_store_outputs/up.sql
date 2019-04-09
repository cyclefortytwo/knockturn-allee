-- Your SQL goes here
ALTER TABLE transactions ADD COLUMN height BIGINT, ADD COLUMN "commit" TEXT;

CREATE UNIQUE INDEX commit_idx ON transactions("commit");

CREATE TABLE current_height ( height BIGINT PRIMARY KEY);
INSERT INTO current_height (height) VALUES (0);
