-- Add migration script here
CREATE TABLE blocks (
	height INTEGER PRIMARY KEY,
	hash TEXT NOT NULL,
	tx_count INTEGER NOT NULL
);

CREATE TABLE transactions (
	id INTEGER PRIMARY KEY,
	block INTEGER NOT NULL REFERENCES blocks(height),
	txid TEXT NOT NULL,
	scalar TEXT NOT NULL
);

CREATE TABLE outputs (
	id INTEGER PRIMARY KEY,
	tx INTEGER NOT NULL REFERENCES transactions(id),
	vout INTEGER NOT NULL,
	value INTEGER NOT NULL,
	script_pub_key TEXT NOT NULL
);
