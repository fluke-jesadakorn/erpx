ALTER TABLE social_post_drafts ADD COLUMN mentions_json TEXT NOT NULL DEFAULT '[]';
ALTER TABLE social_posts ADD COLUMN mentions_json TEXT NOT NULL DEFAULT '[]';
ALTER TABLE social_comments ADD COLUMN mentions_json TEXT NOT NULL DEFAULT '[]';
ALTER TABLE social_comments ADD COLUMN author_kind TEXT NOT NULL DEFAULT 'person' CHECK(author_kind IN ('person','agent'));
ALTER TABLE social_mentions DROP CONSTRAINT social_mentions_target_kind_check;
ALTER TABLE social_mentions ADD CHECK(target_kind IN ('person','record','agent'));
CREATE TABLE social_comment_topics (
 comment_id TEXT NOT NULL REFERENCES social_comments(id) ON DELETE CASCADE,
 tag TEXT NOT NULL REFERENCES social_topics(tag), PRIMARY KEY(comment_id,tag)
);
CREATE INDEX social_comment_topics_tag ON social_comment_topics(tag,comment_id);
CREATE TABLE social_agent_replies (
 id TEXT PRIMARY KEY, post_id TEXT NOT NULL REFERENCES social_posts(id) ON DELETE CASCADE,
 owner_type TEXT NOT NULL CHECK(owner_type IN ('post','comment')), owner_id TEXT NOT NULL,
 company_id TEXT NOT NULL, actor_id TEXT NOT NULL, actor_json TEXT NOT NULL,
 state TEXT NOT NULL DEFAULT 'pending' CHECK(state IN ('pending','running','completed','failed')),
 attempt INTEGER NOT NULL DEFAULT 0, lease_token TEXT, lease_until BIGINT NOT NULL DEFAULT 0,
 error TEXT, created_at BIGINT NOT NULL, updated_at BIGINT NOT NULL,
 UNIQUE(owner_type,owner_id)
);
CREATE INDEX social_agent_replies_pending ON social_agent_replies(state,created_at);
ALTER TABLE social_ai_drafts DROP CONSTRAINT social_ai_drafts_kind_check;
ALTER TABLE social_ai_drafts ADD CHECK(kind IN ('summary','task','reply'));
