-- One-release compatibility links for live workflows that predate the
-- canonical attachment-set API. The legacy records remain the workflow-facing
-- index, while bytes and lifecycle state live on the canonical source/link.
ALTER TABLE messaging_files
  ADD COLUMN attachment_set_id TEXT REFERENCES attachment_sets(id),
  ADD COLUMN attachment_link_id TEXT REFERENCES attachment_links(id),
  ADD COLUMN attachment_source_id TEXT REFERENCES attachments(id);
CREATE UNIQUE INDEX messaging_files_attachment_link
  ON messaging_files(attachment_link_id) WHERE attachment_link_id IS NOT NULL;

ALTER TABLE legal_documents
  ADD COLUMN attachment_link_id TEXT REFERENCES attachment_links(id),
  ADD COLUMN attachment_source_id TEXT REFERENCES attachments(id);
CREATE UNIQUE INDEX legal_documents_attachment_link
  ON legal_documents(attachment_link_id) WHERE attachment_link_id IS NOT NULL;

ALTER TABLE legal_external_uploads
  ADD COLUMN attachment_link_id TEXT REFERENCES attachment_links(id),
  ADD COLUMN attachment_source_id TEXT REFERENCES attachments(id);
CREATE UNIQUE INDEX legal_external_uploads_attachment_link
  ON legal_external_uploads(attachment_link_id) WHERE attachment_link_id IS NOT NULL;

-- Historical rows may acquire their canonical pointers exactly once. All
-- original Legal document/evidence fields remain immutable.
CREATE OR REPLACE FUNCTION legal_document_immutable() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN
 IF TG_OP='DELETE' THEN RAISE EXCEPTION 'LEGAL_DOCUMENT_PRESERVATION_REQUIRED'; END IF;
 IF (to_jsonb(NEW)-'locked'-'attachment_link_id'-'attachment_source_id') IS DISTINCT FROM (to_jsonb(OLD)-'locked'-'attachment_link_id'-'attachment_source_id')
   OR (OLD.locked AND NOT NEW.locked)
   OR (NEW.attachment_link_id IS DISTINCT FROM OLD.attachment_link_id AND NOT (OLD.attachment_link_id IS NULL AND NEW.attachment_link_id IS NOT NULL))
   OR (NEW.attachment_source_id IS DISTINCT FROM OLD.attachment_source_id AND NOT (OLD.attachment_source_id IS NULL AND NEW.attachment_source_id IS NOT NULL))
 THEN RAISE EXCEPTION 'LEGAL_DOCUMENT_IMMUTABLE'; END IF;
 RETURN NEW; END $$;

CREATE OR REPLACE FUNCTION legal_external_upload_canonical_link() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN
 IF TG_OP='DELETE' THEN RAISE EXCEPTION 'LEGAL_EVIDENCE_IMMUTABLE'; END IF;
 IF (to_jsonb(NEW)-'attachment_link_id'-'attachment_source_id') IS DISTINCT FROM (to_jsonb(OLD)-'attachment_link_id'-'attachment_source_id')
   OR (NEW.attachment_link_id IS DISTINCT FROM OLD.attachment_link_id AND NOT (OLD.attachment_link_id IS NULL AND NEW.attachment_link_id IS NOT NULL))
   OR (NEW.attachment_source_id IS DISTINCT FROM OLD.attachment_source_id AND NOT (OLD.attachment_source_id IS NULL AND NEW.attachment_source_id IS NOT NULL))
 THEN RAISE EXCEPTION 'LEGAL_EVIDENCE_IMMUTABLE'; END IF;
 RETURN NEW; END $$;
DROP TRIGGER legal_external_upload_immutable ON legal_external_uploads;
CREATE TRIGGER legal_external_upload_immutable BEFORE UPDATE OR DELETE ON legal_external_uploads FOR EACH ROW EXECUTE FUNCTION legal_external_upload_canonical_link();
