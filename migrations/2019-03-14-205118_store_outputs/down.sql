-- This file should undo anything in `up.sql`
DROP INDEX  commit_idx;

ALTER TABLE transactions DROP COLUMN height , DROP COLUMN "commit" ;

DROP TABLE current_height;
