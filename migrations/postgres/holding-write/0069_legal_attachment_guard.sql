CREATE OR REPLACE FUNCTION legal_attachment_preserved() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE source text; BEGIN
 PERFORM pg_advisory_xact_lock(hashtextextended('legal-files',0));
 -- Both tables use this trigger; avoid compiling a field absent on attachments.
 source := CASE WHEN TG_TABLE_NAME='attachments' THEN to_jsonb(OLD)->>'id' ELSE to_jsonb(OLD)->>'attachment_id' END;
 IF EXISTS(SELECT 1 FROM legal_file_preservations p JOIN legal_hold_targets t ON t.item_id=p.item_id JOIN legal_holds h ON h.id=t.hold_id WHERE p.source_type='attachment' AND p.source_id=source AND h.state!='released') THEN
  IF TG_OP='DELETE' OR NEW.object_key IS DISTINCT FROM OLD.object_key THEN RAISE EXCEPTION 'LEGAL_HOLD_BLOCKS_FILE_MUTATION'; END IF;
 END IF;
 IF TG_OP='DELETE' THEN RETURN OLD; END IF; RETURN NEW;
END $$;
