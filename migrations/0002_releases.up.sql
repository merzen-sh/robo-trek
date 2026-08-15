CREATE TABLE IF NOT EXISTS releases (
    version TEXT PRIMARY KEY,
    png     BLOB NOT NULL
);