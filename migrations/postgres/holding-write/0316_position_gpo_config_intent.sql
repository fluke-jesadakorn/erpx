-- A pending configuration intent does not prove ERPX created the Directory
-- setting. Only read-back can promote it to confirmed ownership.
ALTER TABLE position_gpo_config_ownership ADD COLUMN status text NOT NULL DEFAULT 'confirmed'
 CHECK(status IN ('pending','confirmed'));
