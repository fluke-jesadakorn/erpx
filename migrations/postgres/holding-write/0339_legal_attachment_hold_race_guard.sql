-- Serialize Legal preservation expansion with canonical cleanup. A hold may
-- protect an existing source, but it must never be created after that source
-- or one of its physical derivatives has already been destroyed.
CREATE OR REPLACE FUNCTION legal_item_has_destroyed_attachment(target_item TEXT) RETURNS BOOLEAN
LANGUAGE sql STABLE AS $$
  SELECT EXISTS(
    SELECT 1
    FROM (
      SELECT preservation.source_id attachment_id
      FROM legal_file_preservations preservation
      WHERE preservation.item_id=target_item AND preservation.source_type='attachment'
      UNION
      SELECT link.attachment_id
      FROM attachment_sets set_row
      JOIN attachment_links link ON link.set_id=set_row.id
      WHERE set_row.target_type='legal_item' AND set_row.target_id=target_item
        AND link.attachment_id IS NOT NULL AND link.state IN ('draft','bound')
    ) protected
    LEFT JOIN attachments source ON source.id=protected.attachment_id
    WHERE source.id IS NULL OR source.source_state='destroyed'
      OR EXISTS(
        SELECT 1 FROM attachment_storage_ledger storage
        WHERE storage.attachment_id=protected.attachment_id AND storage.state='destroyed'
      )
  )
$$;

CREATE OR REPLACE FUNCTION legal_hold_target_source_guard() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  PERFORM pg_advisory_xact_lock(hashtextextended('legal-files',0));
  IF legal_item_has_destroyed_attachment(NEW.item_id) THEN
    RAISE EXCEPTION 'LEGAL_HOLD_SOURCE_ALREADY_DESTROYED';
  END IF;
  RETURN NEW;
END $$;
CREATE TRIGGER legal_hold_target_source_guard
  BEFORE INSERT OR UPDATE ON legal_hold_targets
  FOR EACH ROW EXECUTE FUNCTION legal_hold_target_source_guard();

CREATE OR REPLACE FUNCTION legal_file_preservation_source_guard() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  PERFORM pg_advisory_xact_lock(hashtextextended('legal-files',0));
  IF NEW.source_type='attachment' AND (
    NOT EXISTS(SELECT 1 FROM attachments source WHERE source.id=NEW.source_id AND source.source_state<>'destroyed')
    OR EXISTS(SELECT 1 FROM attachment_storage_ledger storage WHERE storage.attachment_id=NEW.source_id AND storage.state='destroyed')
  ) THEN
    RAISE EXCEPTION 'LEGAL_HOLD_SOURCE_ALREADY_DESTROYED';
  END IF;
  RETURN NEW;
END $$;
CREATE TRIGGER legal_file_preservation_source_guard
  BEFORE INSERT OR UPDATE ON legal_file_preservations
  FOR EACH ROW EXECUTE FUNCTION legal_file_preservation_source_guard();

CREATE OR REPLACE FUNCTION legal_attachment_preserved() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE source TEXT; active_preservation BOOLEAN; BEGIN
 PERFORM pg_advisory_xact_lock(hashtextextended('legal-files',0));
 source := CASE WHEN TG_TABLE_NAME='attachments' THEN to_jsonb(OLD)->>'id' ELSE to_jsonb(OLD)->>'attachment_id' END;
 active_preservation :=
   EXISTS(
     SELECT 1 FROM legal_file_preservations preservation
     JOIN legal_hold_targets target ON target.item_id=preservation.item_id
     JOIN legal_holds hold ON hold.id=target.hold_id
     WHERE preservation.source_type='attachment' AND preservation.source_id=source AND hold.state<>'released'
   )
   OR EXISTS(
     SELECT 1 FROM attachment_links link
     JOIN attachment_sets set_row ON set_row.id=link.set_id
     JOIN legal_hold_targets target ON target.item_id=set_row.target_id
     JOIN legal_holds hold ON hold.id=target.hold_id
     WHERE link.attachment_id=source AND set_row.target_type='legal_item' AND hold.state<>'released'
   );
 IF active_preservation AND (
   TG_OP='DELETE'
   OR to_jsonb(NEW)->>'object_key' IS DISTINCT FROM to_jsonb(OLD)->>'object_key'
   OR (TG_TABLE_NAME='attachments' AND to_jsonb(NEW)->>'source_state'='destroyed')
 ) THEN
   RAISE EXCEPTION 'LEGAL_HOLD_BLOCKS_FILE_MUTATION';
 END IF;
 IF TG_OP='DELETE' THEN RETURN OLD; END IF;
 RETURN NEW;
END $$;

CREATE OR REPLACE FUNCTION legal_attachment_storage_preserved() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF NEW.state IS DISTINCT FROM OLD.state AND NEW.state='destroyed' THEN
    PERFORM pg_advisory_xact_lock(hashtextextended('legal-files',0));
    IF EXISTS(
      SELECT 1 FROM legal_file_preservations preservation
      JOIN legal_hold_targets target ON target.item_id=preservation.item_id
      JOIN legal_holds hold ON hold.id=target.hold_id
      WHERE preservation.source_type='attachment' AND preservation.source_id=NEW.attachment_id AND hold.state<>'released'
    ) OR EXISTS(
      SELECT 1 FROM attachment_links link
      JOIN attachment_sets set_row ON set_row.id=link.set_id
      JOIN legal_hold_targets target ON target.item_id=set_row.target_id
      JOIN legal_holds hold ON hold.id=target.hold_id
      WHERE link.attachment_id=NEW.attachment_id AND set_row.target_type='legal_item' AND hold.state<>'released'
    ) THEN
      RAISE EXCEPTION 'LEGAL_HOLD_BLOCKS_FILE_MUTATION';
    END IF;
  END IF;
  RETURN NEW;
END $$;
CREATE TRIGGER legal_attachment_storage_preserved
  BEFORE UPDATE OF state ON attachment_storage_ledger
  FOR EACH ROW EXECUTE FUNCTION legal_attachment_storage_preserved();
