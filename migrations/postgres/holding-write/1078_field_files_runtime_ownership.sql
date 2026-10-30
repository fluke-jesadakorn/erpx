-- Runtime ownership derives only from persisted Files lineage. Any Field
-- marker quarantines the source from the legacy claimant, even when the rest
-- of its relationship is malformed. This changes no existing rows.
CREATE FUNCTION erpx_files_field_attachment_owned(source_id text)
RETURNS boolean LANGUAGE sql STABLE AS $$
 SELECT EXISTS (
   SELECT 1 FROM attachments a
   LEFT JOIN upload_intents u ON u.id=a.upload_id
   LEFT JOIN attachment_sets s ON s.id=u.attachment_set_id
   WHERE a.id=source_id AND (a.owner_type='field_service_visit'
     OR u.owner_type='field_service_visit' OR s.target_type='field_service_visit')
 ) OR EXISTS (
   SELECT 1 FROM attachment_links l
   LEFT JOIN attachment_sets s ON s.id=l.set_id
   LEFT JOIN upload_intents u ON u.id=l.upload_intent_id
   LEFT JOIN attachment_sets us ON us.id=u.attachment_set_id
   WHERE l.attachment_id=source_id AND (s.target_type='field_service_visit'
     OR u.owner_type='field_service_visit' OR us.target_type='field_service_visit')
 )
$$;

-- Operational fairness only: a cursor never changes a file or scan verdict.
-- A malformed candidate remains owned and visible, while later candidates get
-- an opportunity after process restarts. Selection is revalidated by claim.
CREATE TABLE field_file_scan_cursor (
  company_id TEXT PRIMARY KEY,
  after_created_at BIGINT NOT NULL,
  after_job_id TEXT NOT NULL
);

-- Source-level serialization for ownership classification; metadata only.
CREATE TABLE field_file_scan_lineage_epoch (
  attachment_id TEXT PRIMARY KEY,
  epoch BIGINT NOT NULL DEFAULT 1 CHECK (epoch > 0)
);
INSERT INTO field_file_scan_lineage_epoch(attachment_id)
SELECT id FROM attachments ON CONFLICT DO NOTHING;
INSERT INTO field_file_scan_lineage_epoch(attachment_id)
SELECT DISTINCT attachment_id FROM attachment_links WHERE attachment_id IS NOT NULL ON CONFLICT DO NOTHING;

CREATE FUNCTION erpx_files_lock_scan_lineage(source_id text) RETURNS void LANGUAGE plpgsql AS $$
BEGIN
  INSERT INTO field_file_scan_lineage_epoch(attachment_id) VALUES(source_id) ON CONFLICT DO NOTHING;
  PERFORM 1 FROM field_file_scan_lineage_epoch WHERE attachment_id=source_id FOR UPDATE;
END $$;

-- A legacy claimant locks the job and source epoch before making a fresh
-- ownership decision. The prefilter avoids locking unrelated Field backlog;
-- the post-lock test is authoritative, including malformed Field lineage.
CREATE FUNCTION erpx_files_claim_legacy_scan(now_ms bigint, until_ms bigint, lease text, source_filter text)
RETURNS SETOF attachment_scan_attempts LANGUAGE plpgsql VOLATILE AS $$
DECLARE candidate attachment_scan_attempts%ROWTYPE;
BEGIN
 FOR candidate IN SELECT j.* FROM attachment_scan_attempts j
   WHERE j.state IN ('pending','running') AND j.created_at<=now_ms
     AND (j.claim_until IS NULL OR j.claim_until<now_ms)
     AND (source_filter IS NULL OR j.attachment_id=source_filter)
     AND NOT erpx_files_field_attachment_owned(j.attachment_id)
   ORDER BY j.created_at,j.id FOR UPDATE OF j SKIP LOCKED
 LOOP
   PERFORM erpx_files_lock_scan_lineage(candidate.attachment_id);
   IF NOT erpx_files_field_attachment_owned(candidate.attachment_id) THEN
     RETURN QUERY UPDATE attachment_scan_attempts j SET state='running',claimed_at=now_ms,claim_until=until_ms,lease_token=lease WHERE j.id=candidate.id RETURNING j.*;
     RETURN;
   END IF;
 END LOOP;
 RETURN;
END $$;

-- Ownership changes serialize with a scan claim and cannot reclassify a source
-- while that attempt is processing outside its transaction. Ordinary link/set
-- state, captions and expiry changes do not enter these guards.
CREATE FUNCTION erpx_files_bump_scan_lineage(source_id text) RETURNS void LANGUAGE plpgsql AS $$
BEGIN
 IF source_id IS NULL THEN RETURN; END IF;
 PERFORM erpx_files_lock_scan_lineage(source_id);
 IF EXISTS (SELECT 1 FROM attachment_scan_attempts WHERE attachment_id=source_id AND state='running') THEN
   RAISE EXCEPTION 'Field scan ownership cannot change during an active scan'
     USING ERRCODE='23514', CONSTRAINT='field_scan_ownership_active';
 END IF;
 UPDATE field_file_scan_lineage_epoch SET epoch=epoch+1 WHERE attachment_id=source_id;
END $$;

CREATE FUNCTION erpx_files_guard_field_set_lineage() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE source_id text;
BEGIN
 IF TG_OP='UPDATE' AND NEW.target_type IS NOT DISTINCT FROM OLD.target_type THEN RETURN NEW; END IF;
 IF (TG_OP<>'INSERT' AND OLD.target_type='field_service_visit') OR (TG_OP<>'DELETE' AND NEW.target_type='field_service_visit') THEN
   FOR source_id IN SELECT DISTINCT id FROM (
     SELECT l.attachment_id AS id FROM attachment_links l WHERE l.set_id=COALESCE(NEW.id,OLD.id)
     UNION SELECT a.id FROM upload_intents u JOIN attachments a ON a.upload_id=u.id WHERE u.attachment_set_id=COALESCE(NEW.id,OLD.id)
   ) sources WHERE id IS NOT NULL ORDER BY id LOOP PERFORM erpx_files_bump_scan_lineage(source_id); END LOOP;
 END IF;
 IF TG_OP='DELETE' THEN RETURN OLD; END IF; RETURN NEW;
END $$;
CREATE TRIGGER field_scan_set_lineage AFTER UPDATE OF target_type OR DELETE ON attachment_sets
 FOR EACH ROW EXECUTE FUNCTION erpx_files_guard_field_set_lineage();

CREATE FUNCTION erpx_files_guard_field_link_lineage() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE old_field boolean := false; new_field boolean := false; source_id text;
BEGIN
 IF TG_OP='UPDATE' AND ROW(NEW.set_id,NEW.attachment_id,NEW.upload_intent_id) IS NOT DISTINCT FROM ROW(OLD.set_id,OLD.attachment_id,OLD.upload_intent_id) THEN RETURN NEW; END IF;
 IF TG_OP<>'INSERT' THEN SELECT EXISTS(SELECT 1 FROM attachment_sets s LEFT JOIN upload_intents u ON u.id=OLD.upload_intent_id LEFT JOIN attachment_sets us ON us.id=u.attachment_set_id WHERE s.id=OLD.set_id AND (s.target_type='field_service_visit' OR u.owner_type='field_service_visit' OR us.target_type='field_service_visit')) INTO old_field; END IF;
 IF TG_OP<>'DELETE' THEN SELECT EXISTS(SELECT 1 FROM attachment_sets s LEFT JOIN upload_intents u ON u.id=NEW.upload_intent_id LEFT JOIN attachment_sets us ON us.id=u.attachment_set_id WHERE s.id=NEW.set_id AND (s.target_type='field_service_visit' OR u.owner_type='field_service_visit' OR us.target_type='field_service_visit')) INTO new_field; END IF;
 IF old_field OR new_field THEN
   FOR source_id IN SELECT DISTINCT id FROM (SELECT CASE WHEN TG_OP<>'INSERT' THEN OLD.attachment_id END AS id UNION SELECT CASE WHEN TG_OP<>'DELETE' THEN NEW.attachment_id END) sources WHERE id IS NOT NULL ORDER BY id LOOP PERFORM erpx_files_bump_scan_lineage(source_id); END LOOP;
 END IF;
 IF TG_OP='DELETE' THEN RETURN OLD; END IF; RETURN NEW;
END $$;
CREATE TRIGGER field_scan_link_lineage AFTER INSERT OR UPDATE OF set_id,attachment_id,upload_intent_id OR DELETE ON attachment_links
 FOR EACH ROW EXECUTE FUNCTION erpx_files_guard_field_link_lineage();

CREATE FUNCTION erpx_files_guard_field_upload_lineage() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE old_field boolean := false; new_field boolean := false; source_id text;
BEGIN
 IF TG_OP='UPDATE' AND ROW(NEW.owner_type,NEW.attachment_set_id) IS NOT DISTINCT FROM ROW(OLD.owner_type,OLD.attachment_set_id) THEN RETURN NEW; END IF;
 IF TG_OP<>'INSERT' THEN SELECT OLD.owner_type='field_service_visit' OR EXISTS(SELECT 1 FROM attachment_sets s WHERE s.id=OLD.attachment_set_id AND s.target_type='field_service_visit') INTO old_field; END IF;
 IF TG_OP<>'DELETE' THEN SELECT NEW.owner_type='field_service_visit' OR EXISTS(SELECT 1 FROM attachment_sets s WHERE s.id=NEW.attachment_set_id AND s.target_type='field_service_visit') INTO new_field; END IF;
 IF old_field OR new_field THEN
   FOR source_id IN SELECT DISTINCT id FROM (SELECT a.id FROM attachments a WHERE a.upload_id=COALESCE(NEW.id,OLD.id) UNION SELECT l.attachment_id FROM attachment_links l WHERE l.upload_intent_id=COALESCE(NEW.id,OLD.id)) sources WHERE id IS NOT NULL ORDER BY id LOOP PERFORM erpx_files_bump_scan_lineage(source_id); END LOOP;
 END IF;
 IF TG_OP='DELETE' THEN RETURN OLD; END IF; RETURN NEW;
END $$;
CREATE TRIGGER field_scan_upload_lineage AFTER UPDATE OF owner_type,attachment_set_id OR DELETE ON upload_intents
 FOR EACH ROW EXECUTE FUNCTION erpx_files_guard_field_upload_lineage();

-- Every legacy effect batch reacquires job then lineage in the same order as
-- the claimant. A changed owner/lease aborts the batch rather than fabricating
-- a scanner failure or retry for a source now owned by native Files.
CREATE FUNCTION erpx_files_assert_legacy_scan(source_id text, job_id text, lease text)
RETURNS boolean LANGUAGE plpgsql VOLATILE AS $$
BEGIN
 PERFORM 1 FROM attachment_scan_attempts j WHERE j.id=job_id AND j.attachment_id=source_id AND j.state='running' AND j.lease_token=lease FOR UPDATE;
 IF NOT FOUND THEN RAISE EXCEPTION 'Legacy scan lease changed' USING ERRCODE='23514',CONSTRAINT='field_scan_legacy_lease_changed'; END IF;
 PERFORM erpx_files_lock_scan_lineage(source_id);
 IF erpx_files_field_attachment_owned(source_id) THEN RAISE EXCEPTION 'Field scan is native-owned' USING ERRCODE='23514',CONSTRAINT='field_scan_native_owned'; END IF;
 RETURN true;
END $$;

-- The cleanup worker holds this epoch for its complete transactional mutation
-- and physical deletion. Field lineage changes serialize through the same row.
CREATE FUNCTION erpx_files_legacy_cleanup_source_allowed(source_id text) RETURNS boolean LANGUAGE plpgsql VOLATILE AS $$
BEGIN
 IF source_id IS NULL THEN RETURN true; END IF;
 PERFORM erpx_files_lock_scan_lineage(source_id);
 RETURN NOT erpx_files_field_attachment_owned(source_id);
END $$;

CREATE FUNCTION erpx_files_legacy_cleanup_set_allowed(input_set_id text) RETURNS boolean LANGUAGE plpgsql VOLATILE AS $$
DECLARE row_source text;
BEGIN
 IF NOT EXISTS(SELECT 1 FROM attachment_sets s WHERE s.id=input_set_id AND s.target_type <> 'field_service_visit') THEN RETURN false; END IF;
 IF EXISTS(SELECT 1 FROM upload_intents u WHERE u.attachment_set_id=input_set_id AND u.owner_type='field_service_visit') THEN RETURN false; END IF;
 IF EXISTS(SELECT 1 FROM attachment_links l JOIN upload_intents u ON u.id=l.upload_intent_id LEFT JOIN attachment_sets s ON s.id=u.attachment_set_id WHERE l.set_id=input_set_id AND (u.owner_type='field_service_visit' OR s.target_type='field_service_visit')) THEN RETURN false; END IF;
 FOR row_source IN SELECT DISTINCT id FROM (
   SELECT l.attachment_id id FROM attachment_links l WHERE l.set_id=input_set_id
   UNION SELECT a.id FROM upload_intents u JOIN attachments a ON a.upload_id=u.id WHERE u.attachment_set_id=input_set_id
 ) sources WHERE id IS NOT NULL ORDER BY id LOOP
   IF NOT erpx_files_legacy_cleanup_source_allowed(row_source) THEN RETURN false; END IF;
 END LOOP;
 RETURN true;
END $$;

-- Transport selection is not a writer fence. The writer calls this in the
-- same transaction as its effects, including replay, and fails closed.
CREATE FUNCTION erpx_files_assert_legacy_writer(resource_kind text, resource_id text) RETURNS boolean LANGUAGE plpgsql VOLATILE AS $$
DECLARE set_key text; source_key text;
BEGIN
 IF resource_kind='set' THEN set_key:=resource_id;
 ELSIF resource_kind='upload' THEN
   source_key:='attachment-' || resource_id;
   SELECT attachment_set_id INTO set_key FROM upload_intents WHERE id=resource_id FOR UPDATE;
   IF EXISTS(SELECT 1 FROM upload_intents WHERE id=resource_id AND owner_type='field_service_visit') OR EXISTS(SELECT 1 FROM attachment_links l JOIN attachment_sets s ON s.id=l.set_id WHERE l.upload_intent_id=resource_id AND s.target_type='field_service_visit') THEN
     RAISE EXCEPTION 'Field file writer is native' USING ERRCODE='23514',CONSTRAINT='field_file_native_writer'; END IF;
 ELSIF resource_kind='link' THEN
   SELECT set_id,attachment_id INTO set_key,source_key FROM attachment_links WHERE id=resource_id FOR UPDATE;
 ELSIF resource_kind='attachment' THEN source_key:=resource_id;
 ELSE RAISE EXCEPTION 'Unknown file writer resource' USING ERRCODE='22023'; END IF;
 IF source_key IS NOT NULL AND NOT erpx_files_legacy_cleanup_source_allowed(source_key) THEN
   RAISE EXCEPTION 'Field file writer is native' USING ERRCODE='23514',CONSTRAINT='field_file_native_writer'; END IF;
 IF set_key IS NOT NULL THEN
   PERFORM 1 FROM attachment_sets WHERE id=set_key FOR UPDATE;
   IF NOT erpx_files_legacy_cleanup_set_allowed(set_key) THEN
     RAISE EXCEPTION 'Field file writer is native' USING ERRCODE='23514',CONSTRAINT='field_file_native_writer'; END IF;
 END IF;
 RETURN true;
END $$;

-- A key can be shared by historical sources. Guard every persisted source
-- referring to it, not merely the source the legacy cleanup selected.
CREATE FUNCTION erpx_files_legacy_cleanup_object_allowed(owner_company text, object_key text) RETURNS boolean LANGUAGE plpgsql VOLATILE AS $$
DECLARE row_source text;
BEGIN
 FOR row_source IN SELECT DISTINCT id FROM (
   SELECT a.id FROM attachments a WHERE a.company_id=owner_company AND (a.object_key=erpx_files_legacy_cleanup_object_allowed.object_key OR a.preview_object_key=erpx_files_legacy_cleanup_object_allowed.object_key)
   UNION SELECT l.attachment_id FROM attachment_storage_ledger l WHERE l.company_id=owner_company AND l.object_key=erpx_files_legacy_cleanup_object_allowed.object_key
 ) sources WHERE id IS NOT NULL ORDER BY id LOOP
   IF NOT erpx_files_legacy_cleanup_source_allowed(row_source) THEN RETURN false; END IF;
 END LOOP;
 RETURN true;
END $$;
