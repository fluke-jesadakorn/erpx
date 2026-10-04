ALTER TABLE attachment_orphan_preservations ADD COLUMN preservation_mode TEXT NOT NULL DEFAULT 'local-directory'
  CHECK(preservation_mode IN ('local-directory','file-store-sealed'));

CREATE FUNCTION reject_preserved_orphan_binding() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF NEW.object_key IS NOT NULL AND EXISTS(SELECT 1 FROM attachment_orphan_preservations p
   WHERE p.object_key=NEW.object_key AND NOT EXISTS(SELECT 1 FROM attachment_orphan_preservation_events e WHERE e.preservation_id=p.id AND e.event_kind='restored')) THEN
  RAISE EXCEPTION 'FILE_PRESERVED_PENDING_OWNER' USING ERRCODE='23514';
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER reject_preserved_orphan_attachment BEFORE INSERT OR UPDATE OF object_key ON attachments
  FOR EACH ROW EXECUTE FUNCTION reject_preserved_orphan_binding();
CREATE TRIGGER reject_preserved_orphan_derivative BEFORE INSERT OR UPDATE OF object_key ON attachment_derivatives
  FOR EACH ROW EXECUTE FUNCTION reject_preserved_orphan_binding();
