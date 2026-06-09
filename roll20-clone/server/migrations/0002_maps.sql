-- Maps are stored as scalar metadata columns plus a single JSON `content` blob
-- holding the groups and standalone shapes. The recursive boolean-operator tree
-- round-trips through serde, so a relational decomposition buys us nothing here:
-- a map is always loaded and rendered as a unit.
CREATE TABLE maps (
    id               TEXT PRIMARY KEY,
    name             TEXT NOT NULL,
    width            INTEGER NOT NULL,
    height           INTEGER NOT NULL,
    grid_size        REAL NOT NULL,
    grid_unit        TEXT NOT NULL,
    background_color TEXT NOT NULL,
    grid_color       TEXT NOT NULL,
    content          TEXT NOT NULL,   -- JSON: {"groups":[...],"shapes":[...]}
    updated_at       INTEGER NOT NULL
);
