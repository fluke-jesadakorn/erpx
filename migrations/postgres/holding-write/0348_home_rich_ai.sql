ALTER TABLE ai_company_settings ADD COLUMN home_ai_name TEXT NOT NULL DEFAULT 'AI'
  CHECK(length(home_ai_name) BETWEEN 1 AND 40 AND home_ai_name !~ '[@\r\n]');
CREATE TABLE ai_home_name_history(company_id TEXT NOT NULL,name TEXT NOT NULL,PRIMARY KEY(company_id,name));
ALTER TABLE social_post_drafts DROP CONSTRAINT social_post_drafts_body_check;
ALTER TABLE social_post_drafts ADD CONSTRAINT social_post_drafts_body_check CHECK(length(body)<=30000);
ALTER TABLE social_posts DROP CONSTRAINT social_posts_body_check;
ALTER TABLE social_posts ADD CONSTRAINT social_posts_body_check CHECK(
  length(body) BETWEEN 1 AND 30000 OR
  (body='' AND kind='team' AND content_json IS NOT NULL AND content_json::jsonb @> '[{"type":"image"}]'::jsonb));
ALTER TABLE social_post_drafts ADD COLUMN rich_content_json TEXT
  CHECK(rich_content_json IS NULL OR (octet_length(rich_content_json) <= 1048576 AND erpx_json_valid(rich_content_json)));
ALTER TABLE social_post_drafts ADD COLUMN comment_parent_post_id TEXT REFERENCES social_posts(id);
ALTER TABLE social_post_drafts ADD COLUMN published_comment_id TEXT REFERENCES social_comments(id);
ALTER TABLE social_post_drafts ADD COLUMN cancelled_at BIGINT;
CREATE INDEX social_comment_upload_drafts_parent ON social_post_drafts(comment_parent_post_id) WHERE comment_parent_post_id IS NOT NULL;
ALTER TABLE social_posts ADD COLUMN rich_content_json TEXT
  CHECK(rich_content_json IS NULL OR (octet_length(rich_content_json) <= 1048576 AND erpx_json_valid(rich_content_json)));
ALTER TABLE social_comments ADD COLUMN rich_content_json TEXT
  CHECK(rich_content_json IS NULL OR (octet_length(rich_content_json) <= 1048576 AND erpx_json_valid(rich_content_json)));
ALTER TABLE social_comments ADD COLUMN comment_upload_draft_id TEXT REFERENCES social_post_drafts(id);
ALTER TABLE social_comments DROP CONSTRAINT social_comments_body_check;
ALTER TABLE social_comments ADD CONSTRAINT social_comments_body_check CHECK(length(body) BETWEEN 1 AND 10000 OR (body='' AND comment_upload_draft_id IS NOT NULL));
ALTER TABLE social_post_revisions ADD COLUMN rich_content_json TEXT
  CHECK(rich_content_json IS NULL OR (octet_length(rich_content_json) <= 1048576 AND erpx_json_valid(rich_content_json)));
ALTER TABLE social_agent_replies ADD COLUMN error_code TEXT;
ALTER TABLE social_agent_replies ADD COLUMN requester_name TEXT;
UPDATE social_agent_replies r SET requester_name=COALESCE(
  (SELECT c.author_name FROM social_comments c WHERE r.owner_type='comment' AND c.id=r.owner_id),
  (SELECT p.author_name FROM social_posts p WHERE r.owner_type='post' AND p.id=r.owner_id),
  r.actor_id);
