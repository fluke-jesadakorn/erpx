ALTER TABLE social_post_drafts
  ADD COLUMN edit_post_id TEXT REFERENCES social_posts(id),
  ADD COLUMN edit_post_version BIGINT,
  ADD COLUMN edit_saved_version BIGINT;

ALTER TABLE social_post_drafts
  ADD CONSTRAINT social_post_drafts_edit_post_version_check CHECK (
    (edit_post_id IS NULL AND edit_post_version IS NULL)
    OR (edit_post_id IS NOT NULL AND edit_post_version IS NOT NULL AND edit_post_version > 0)
  ),
  ADD CONSTRAINT social_post_drafts_edit_saved_version_check CHECK (
    edit_saved_version IS NULL OR edit_saved_version > 0
  );

CREATE INDEX social_post_edit_upload_drafts_post
  ON social_post_drafts(edit_post_id, edit_post_version)
  WHERE edit_post_id IS NOT NULL;

UPDATE social_comments c
SET parent_comment_id = r.owner_id
FROM social_agent_replies r
WHERE r.owner_type = 'comment'
  AND c.id = 'reply-' || r.id
  AND c.post_id = r.post_id
  AND c.author_kind = 'agent'
  AND c.parent_comment_id IS DISTINCT FROM r.owner_id;
