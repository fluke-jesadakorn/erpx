-- Keep every historical image revision; canonical sources are populated by the
-- byte/checksum-verifying backfill and remain quarantined until scanner review.
ALTER TABLE pos_product_images
  ADD COLUMN attachment_source_id TEXT REFERENCES attachments(id),
  ADD COLUMN attachment_link_id TEXT REFERENCES attachment_links(id),
  ADD COLUMN attachment_set_id TEXT REFERENCES attachment_sets(id),
  ADD COLUMN thumbnail_checksum_sha256 TEXT;
CREATE UNIQUE INDEX pos_image_canonical_source ON pos_product_images(attachment_source_id)
  WHERE attachment_source_id IS NOT NULL;
