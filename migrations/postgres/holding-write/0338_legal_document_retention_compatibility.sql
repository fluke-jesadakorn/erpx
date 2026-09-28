-- 0334 added one-time canonical source pointers to Legal documents. Preserve
-- the controlled-destruction exception introduced by 0064 while retaining
-- those pointer and immutable-history rules.
CREATE OR REPLACE FUNCTION legal_document_immutable() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN
 IF TG_OP='DELETE' THEN RAISE EXCEPTION 'LEGAL_DOCUMENT_PRESERVATION_REQUIRED'; END IF;
 IF NEW.body='' AND NEW.object_key IS NULL
   AND (to_jsonb(NEW)-'body'-'object_key')=(to_jsonb(OLD)-'body'-'object_key')
   AND EXISTS(SELECT 1 FROM legal_destructions x WHERE x.item_id=OLD.item_id AND x.state='executing')
   AND NOT EXISTS(SELECT 1 FROM legal_hold_targets t JOIN legal_holds h ON h.id=t.hold_id WHERE t.item_id=OLD.item_id AND h.state!='released')
 THEN RETURN NEW; END IF;
 IF (to_jsonb(NEW)-'locked'-'attachment_link_id'-'attachment_source_id') IS DISTINCT FROM (to_jsonb(OLD)-'locked'-'attachment_link_id'-'attachment_source_id')
   OR (OLD.locked AND NOT NEW.locked)
   OR (NEW.attachment_link_id IS DISTINCT FROM OLD.attachment_link_id AND NOT (OLD.attachment_link_id IS NULL AND NEW.attachment_link_id IS NOT NULL))
   OR (NEW.attachment_source_id IS DISTINCT FROM OLD.attachment_source_id AND NOT (OLD.attachment_source_id IS NULL AND NEW.attachment_source_id IS NOT NULL))
 THEN RAISE EXCEPTION 'LEGAL_DOCUMENT_IMMUTABLE'; END IF;
 RETURN NEW;
END $$;
