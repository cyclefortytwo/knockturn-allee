-- This file should undo anything in `up.sql`

ALTER TABLE transactions ALTER  COLUMN status SET DATA TYPE SMALLINT USING CASE
	WHEN status = 'new'::transaction_status THEN 1
	WHEN status = 'pending'::transaction_status THEN 2
	WHEN status = 'rejected'::transaction_status THEN 3
	WHEN status = 'in_chain'::transaction_status THEN 4
	WHEN status = 'confirmed'::transaction_status THEN 5
	WHEN status = 'initialized'::transaction_status THEN 6
END;

DROP TYPE transaction_status;
