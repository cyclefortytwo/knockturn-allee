-- Your SQL goes here

CREATE TYPE transaction_status AS ENUM (
	'new',
    'pending',
    'rejected',
    'in_chain',
    'confirmed',
    'initialized'
);

ALTER TABLE transactions ALTER  COLUMN status SET DATA TYPE transaction_status USING  CASE
	WHEN status = 1 THEN 'new'::transaction_status
	WHEN status = 2 THEN 'pending'::transaction_status
	WHEN status = 3 THEN 'rejected'::transaction_status
	WHEN status = 4 THEN 'in_chain'::transaction_status
	WHEN status = 5 THEN 'confirmed'::transaction_status
	WHEN status = 6 THEN 'initialized'::transaction_status
END;
