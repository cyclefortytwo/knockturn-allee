-- Your SQL goes here

ALTER TABLE transactions ADD COLUMN knockturn_fee BIGINT;
ALTER TABLE transactions ADD COLUMN transfer_fee BIGINT;
ALTER TABLE transactions ADD COLUMN real_transfer_fee BIGINT;
CREATE TYPE transaction_type AS ENUM ('payment', 'payout');
ALTER TABLE transactions ADD COLUMN transaction_type transaction_type NOT NULL DEFAULT 'payment';
