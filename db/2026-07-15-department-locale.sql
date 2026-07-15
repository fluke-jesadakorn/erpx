ALTER TABLE perm.roles
  ADD COLUMN IF NOT EXISTS display_name_de text;

ALTER TABLE customers
  ADD COLUMN IF NOT EXISTS name_de text;

UPDATE perm.roles SET display_name_de = display_name WHERE display_name_de IS NULL;
UPDATE customers SET name_de = name WHERE name_de IS NULL;