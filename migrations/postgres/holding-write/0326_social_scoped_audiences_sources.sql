ALTER TABLE social_post_drafts
  DROP CONSTRAINT IF EXISTS social_post_drafts_audience_kind_check;
ALTER TABLE social_posts
  DROP CONSTRAINT IF EXISTS social_posts_audience_kind_check;

ALTER TABLE social_post_drafts
  ADD CONSTRAINT social_post_drafts_audience_kind_check
  CHECK (audience_kind IN ('company', 'branch', 'department', 'participants', 'team', 'project'));
ALTER TABLE social_posts
  ADD CONSTRAINT social_posts_audience_kind_check
  CHECK (audience_kind IN ('company', 'branch', 'department', 'participants', 'team', 'project'));

ALTER TABLE social_post_drafts ADD COLUMN audience_id TEXT;
ALTER TABLE social_post_drafts ADD COLUMN participants_json TEXT NOT NULL DEFAULT '[]'
  CHECK (erpx_json_valid(participants_json));
ALTER TABLE social_post_drafts ADD COLUMN sources_json TEXT NOT NULL DEFAULT '[]'
  CHECK (erpx_json_valid(sources_json));
ALTER TABLE social_posts ADD COLUMN audience_id TEXT;

CREATE TABLE social_post_participants (
  post_id TEXT NOT NULL REFERENCES social_posts(id),
  user_id TEXT NOT NULL,
  PRIMARY KEY (post_id, user_id)
);
CREATE INDEX social_post_participants_user ON social_post_participants(user_id, post_id);

CREATE TABLE social_post_sources (
  post_id TEXT NOT NULL REFERENCES social_posts(id),
  ordinal INTEGER NOT NULL CHECK (ordinal BETWEEN 0 AND 4),
  source_kind TEXT NOT NULL CHECK (source_kind IN ('staff_task', 'staff_project')),
  source_id TEXT NOT NULL,
  PRIMARY KEY (post_id, ordinal),
  UNIQUE (post_id, source_kind, source_id)
);
CREATE INDEX social_post_sources_target ON social_post_sources(source_kind, source_id);
