-- Existing upload intents retain their original semantics; only Support requires this version.
ALTER TABLE upload_intents ADD COLUMN owner_version BIGINT;
ALTER TABLE upload_intents ADD CONSTRAINT support_upload_version CHECK(owner_type<>'support_ticket' OR (owner_version IS NOT NULL AND owner_version>0));
