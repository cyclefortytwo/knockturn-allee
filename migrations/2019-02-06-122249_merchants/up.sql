CREATE TABLE merchants (
  id TEXT NOT NULL PRIMARY KEY,
  email VARCHAR(100) NOT NULL UNIQUE,
  password VARCHAR(64) NOT NULL, --bcrypt hash
  wallet_url TEXT,
  balance BIGINT NOT NULL DEFAULT 0,
  created_at TIMESTAMP NOT NULL
);
