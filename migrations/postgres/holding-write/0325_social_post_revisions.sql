ALTER TABLE social_posts ADD COLUMN edited_at BIGINT;
ALTER TABLE social_posts ADD COLUMN retracted_at BIGINT;

CREATE TABLE social_post_revisions (
  id TEXT PRIMARY KEY,
  post_id TEXT NOT NULL,
  prior_version BIGINT NOT NULL CHECK (prior_version > 0),
  action TEXT NOT NULL CHECK (action IN ('edit', 'retract')),
  body TEXT NOT NULL,
  content_json TEXT,
  mentions_json TEXT NOT NULL,
  actor_id TEXT NOT NULL,
  created_at BIGINT NOT NULL,
  UNIQUE (post_id, prior_version)
);

CREATE INDEX social_post_revisions_post_idx
  ON social_post_revisions(post_id, prior_version DESC);

ALTER TABLE social_comments ADD COLUMN parent_comment_id TEXT;

CREATE INDEX social_comments_thread_idx
  ON social_comments(post_id, parent_comment_id, created_at);
