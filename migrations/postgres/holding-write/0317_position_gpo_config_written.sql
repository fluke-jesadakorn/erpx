ALTER TABLE position_gpo_config_ownership DROP CONSTRAINT position_gpo_config_ownership_status_check;
ALTER TABLE position_gpo_config_ownership ADD CONSTRAINT position_gpo_config_ownership_status_check
 CHECK(status IN ('pending','written','confirmed'));
