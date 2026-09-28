ALTER TABLE social_post_drafts
  ADD COLUMN audience_kind TEXT CHECK (audience_kind IN ('company', 'branch', 'department'));

ALTER TABLE social_post_drafts
  ADD COLUMN purpose TEXT NOT NULL DEFAULT 'update'
  CHECK (purpose IN ('update', 'plan', 'blocker', 'handover', 'announcement'));

ALTER TABLE social_posts
  ADD COLUMN audience_kind TEXT CHECK (audience_kind IN ('company', 'branch', 'department'));

ALTER TABLE social_posts
  ADD COLUMN purpose TEXT NOT NULL DEFAULT 'update'
  CHECK (purpose IN ('update', 'plan', 'blocker', 'handover', 'announcement'));

-- Existing published posts retain their recorded scope. Unpublished drafts must have
-- their audience chosen again before publishing.
UPDATE social_posts SET audience_kind = CASE
  WHEN department_id IS NOT NULL THEN 'department'
  WHEN branch_id IS NOT NULL THEN 'branch'
  ELSE 'company'
END WHERE audience_kind IS NULL;
