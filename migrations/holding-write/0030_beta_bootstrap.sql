CREATE TABLE beta_bootstrap (
  singleton INTEGER PRIMARY KEY CHECK(singleton=1), holding_id TEXT NOT NULL UNIQUE,
  owner_user_id TEXT NOT NULL, created_at INTEGER NOT NULL
) STRICT;
ALTER TABLE social_post_drafts ADD COLUMN department_id TEXT;
CREATE TRIGGER social_comment_insert_revision AFTER INSERT ON social_comments BEGIN
 UPDATE social_posts SET version=version+1,updated_at=MAX(updated_at,NEW.updated_at) WHERE id=NEW.post_id;
END;
CREATE TRIGGER social_comment_update_revision AFTER UPDATE OF body,state ON social_comments BEGIN
 UPDATE social_posts SET version=version+1,updated_at=MAX(updated_at,NEW.updated_at) WHERE id=NEW.post_id;
 UPDATE social_comments SET version=OLD.version+1 WHERE id=NEW.id;
END;
