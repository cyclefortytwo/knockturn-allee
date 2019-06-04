-- Your SQL goes here

INSERT INTO pg_enum (enumtypid , enumsortorder, enumlabel) 
	SELECT enumtypid, max(enumsortorder) + 1, 'refund' 
		FROM pg_enum WHERE enumtypid = (select oid from pg_type where typname = 'transaction_status') 
		GROUP BY enumtypid;

