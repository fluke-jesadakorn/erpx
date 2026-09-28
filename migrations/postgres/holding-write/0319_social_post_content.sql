ALTER TABLE social_post_drafts
  ADD COLUMN content_json TEXT CHECK(content_json IS NULL OR erpx_json_valid(content_json));

ALTER TABLE social_posts
  ADD COLUMN content_json TEXT CHECK(content_json IS NULL OR erpx_json_valid(content_json));

ALTER TABLE social_posts DROP CONSTRAINT social_posts_body_check;
ALTER TABLE social_posts ADD CONSTRAINT social_posts_body_check CHECK (
  length(body) BETWEEN 1 AND 3000
  OR (
    body = ''
    AND kind = 'team'
    AND content_json IS NOT NULL
    AND content_json::jsonb @> '[{"type":"image"}]'::jsonb
  )
);
