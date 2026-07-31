-- A source revision is only idempotent when the complete immutable-feed
-- payload is the same.  Persist the digest on the write-side identity link so
-- conflicting same-revision observations fail closed instead of being
-- silently discarded.
ALTER TABLE erpx_identity.directory_identity_links
    ADD COLUMN IF NOT EXISTS source_payload_hash TEXT;

ALTER TABLE erpx_identity.directory_identity_links
    DROP CONSTRAINT IF EXISTS directory_identity_links_source_payload_hash_check;
ALTER TABLE erpx_identity.directory_identity_links
    ADD CONSTRAINT directory_identity_links_source_payload_hash_check
    CHECK (source_payload_hash IS NULL OR length(source_payload_hash) = 64);
