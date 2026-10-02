ALTER TABLE organization_drafts ADD COLUMN base_snapshot jsonb NOT NULL DEFAULT '{}'::jsonb;
