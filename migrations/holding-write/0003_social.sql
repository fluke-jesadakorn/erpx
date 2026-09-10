PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS social_posts (
  id TEXT PRIMARY KEY,
  kind TEXT NOT NULL DEFAULT 'team' CHECK(kind IN ('team','record','system')),
  author_id TEXT NOT NULL,
  author_name TEXT NOT NULL,
  company_id TEXT NOT NULL,
  branch_id TEXT,
  body TEXT NOT NULL CHECK(length(body) BETWEEN 1 AND 3000),
  linked_module TEXT,
  linked_record_id TEXT,
  linked_record_label TEXT,
  state TEXT NOT NULL DEFAULT 'published' CHECK(state IN ('published','archived','moderated')),
  version INTEGER NOT NULL DEFAULT 1 CHECK(version > 0),
  last_command_id TEXT,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  CHECK((linked_module IS NULL AND linked_record_id IS NULL AND linked_record_label IS NULL) OR
        (linked_module IS NOT NULL AND linked_record_id IS NOT NULL AND linked_record_label IS NOT NULL))
) STRICT;
CREATE INDEX IF NOT EXISTS social_feed_idx
  ON social_posts(company_id, branch_id, state, created_at DESC);

CREATE TABLE IF NOT EXISTS social_topics (
  tag TEXT PRIMARY KEY CHECK(length(tag) BETWEEN 1 AND 40),
  created_by TEXT NOT NULL,
  created_at INTEGER NOT NULL
) STRICT;

CREATE TABLE IF NOT EXISTS social_post_topics (
  post_id TEXT NOT NULL REFERENCES social_posts(id) ON DELETE CASCADE,
  tag TEXT NOT NULL REFERENCES social_topics(tag),
  PRIMARY KEY(post_id, tag)
) STRICT;
CREATE INDEX IF NOT EXISTS social_topic_posts_idx ON social_post_topics(tag, post_id);

CREATE TABLE IF NOT EXISTS social_comments (
  id TEXT PRIMARY KEY,
  post_id TEXT NOT NULL REFERENCES social_posts(id) ON DELETE CASCADE,
  author_id TEXT NOT NULL,
  author_name TEXT NOT NULL,
  body TEXT NOT NULL CHECK(length(body) BETWEEN 1 AND 1000),
  state TEXT NOT NULL DEFAULT 'published' CHECK(state IN ('published','tombstoned')),
  last_command_id TEXT,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL
) STRICT;
CREATE INDEX IF NOT EXISTS social_comments_post_idx ON social_comments(post_id, created_at);

CREATE TABLE IF NOT EXISTS social_reactions (
  post_id TEXT NOT NULL REFERENCES social_posts(id) ON DELETE CASCADE,
  principal_id TEXT NOT NULL,
  kind TEXT NOT NULL CHECK(kind = 'appreciate'),
  created_at INTEGER NOT NULL,
  PRIMARY KEY(post_id, principal_id, kind)
) STRICT;

CREATE TABLE IF NOT EXISTS social_follows (
  principal_id TEXT NOT NULL,
  target_type TEXT NOT NULL CHECK(target_type IN ('post','person','topic','record')),
  target_id TEXT NOT NULL,
  created_at INTEGER NOT NULL,
  PRIMARY KEY(principal_id, target_type, target_id)
) STRICT;

CREATE TABLE IF NOT EXISTS social_stories (
  id TEXT PRIMARY KEY,
  author_id TEXT NOT NULL,
  author_name TEXT NOT NULL,
  company_id TEXT NOT NULL,
  branch_id TEXT,
  title TEXT NOT NULL CHECK(length(title) BETWEEN 1 AND 80),
  body TEXT NOT NULL CHECK(length(body) BETWEEN 1 AND 500),
  theme TEXT NOT NULL DEFAULT 'sage' CHECK(theme IN ('sage','sunset','sky','plum')),
  state TEXT NOT NULL DEFAULT 'published' CHECK(state IN ('published','archived','moderated')),
  version INTEGER NOT NULL DEFAULT 1 CHECK(version > 0),
  last_command_id TEXT,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL
) STRICT;
CREATE INDEX IF NOT EXISTS social_stories_scope_idx
  ON social_stories(company_id, branch_id, state, created_at DESC);
