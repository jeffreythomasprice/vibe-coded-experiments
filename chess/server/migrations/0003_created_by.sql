ALTER TABLE games ADD COLUMN created_by UUID REFERENCES users(id);
