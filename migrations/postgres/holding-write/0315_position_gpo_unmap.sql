-- Keep an immutable versioned tombstone when IT removes an optional mapping.
ALTER TABLE position_gpo_revisions ALTER COLUMN connection_id DROP NOT NULL;
ALTER TABLE position_gpo_revisions ALTER COLUMN gpo_id DROP NOT NULL;
ALTER TABLE position_gpo_revisions ALTER COLUMN user_filter_group_id DROP NOT NULL;
ALTER TABLE position_gpo_revisions ALTER COLUMN computer_target_ou_id DROP NOT NULL;
ALTER TABLE position_gpo_revisions ALTER COLUMN directory_gpo_version DROP NOT NULL;
