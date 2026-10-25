-- Preserve the logical transactional revision interface while independent PMS
-- Properties stop updating the same physical row under SERIALIZABLE.
-- All original baseline facts remain; no business records/counters are reset.
-- https://www.postgresql.org/docs/18/sql-createview.html
-- https://www.postgresql.org/docs/18/sql-createtrigger.html
ALTER TABLE work_cache_revisions RENAME TO work_cache_revision_baselines;
CREATE TABLE work_cache_revision_scopes (
  table_name text NOT NULL REFERENCES work_cache_revision_baselines(table_name)
    ON UPDATE CASCADE ON DELETE CASCADE,
  property_id text NOT NULL,
  revision bigint NOT NULL DEFAULT 0 CHECK(revision>=0),
  PRIMARY KEY(table_name,property_id)
);
CREATE VIEW work_cache_revisions WITH(security_invoker=true) AS
  SELECT b.table_name,(b.revision+COALESCE(s.revision,0))::bigint revision
  FROM work_cache_revision_baselines b LEFT JOIN (
    SELECT table_name,SUM(revision) revision FROM work_cache_revision_scopes GROUP BY table_name
  ) s ON s.table_name=b.table_name;
ALTER VIEW work_cache_revisions ALTER COLUMN revision SET DEFAULT 0;

-- Keep the existing administrative INSERT/UPDATE/DELETE interface. Coverage
-- uses the physical owner for ON CONFLICT, which an aggregate view cannot use.
CREATE FUNCTION work_cache_revision_edit() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF TG_OP='INSERT' THEN
    INSERT INTO work_cache_revision_baselines(table_name,revision) VALUES(NEW.table_name,NEW.revision);
    RETURN NEW;
  ELSIF TG_OP='DELETE' THEN
    DELETE FROM work_cache_revision_baselines WHERE table_name=OLD.table_name;
    RETURN OLD;
  ELSE
    UPDATE work_cache_revision_baselines SET table_name=NEW.table_name,
      revision=revision+(NEW.revision-OLD.revision) WHERE table_name=OLD.table_name;
    RETURN NEW;
  END IF;
END $$;
CREATE TRIGGER work_cache_revision_edit INSTEAD OF INSERT OR UPDATE OR DELETE
  ON work_cache_revisions FOR EACH ROW EXECUTE FUNCTION work_cache_revision_edit();

CREATE OR REPLACE FUNCTION work_cache_tracks(name text) RETURNS boolean LANGUAGE sql IMMUTABLE AS $$
  SELECT name NOT IN ('work_cache_revisions','work_cache_revision_baselines','work_cache_revision_scopes',
    'erpx_migrations','outbox_events','command_assertions','command_receipts','audit_events','runtime_health',
    'iam_audit','iam_receipts','iam_sync_jobs','um_history','seed_repairs',
    'home_read_cursors','active_work_contexts','work_role_preferences',
    'work_card_preferences','navigation_preferences','pos_audit','pms_jobs',
    'legal_access_events','legal_document_inspections','legal_ai_proposals')
    AND name NOT LIKE '%\_receipts' ESCAPE '\'
$$;

CREATE OR REPLACE FUNCTION work_cache_changed() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE scoped_property text;
BEGIN
  IF TG_OP='UPDATE' AND
    (to_jsonb(OLD)-ARRAY['lease_token','lease_until','claim_until','next_attempt_at','attempts','heartbeat_at'])
    IS NOT DISTINCT FROM
    (to_jsonb(NEW)-ARRAY['lease_token','lease_until','claim_until','next_attempt_at','attempts','heartbeat_at'])
  THEN RETURN NULL; END IF;
  IF TG_OP<>'TRUNCATE' AND current_setting('erpx.pms_write',true)='on' THEN
    scoped_property:=NULLIF(current_setting('erpx.pms_property',true),'');
  END IF;
  IF scoped_property IS NULL THEN
    UPDATE work_cache_revision_baselines SET revision=revision+1 WHERE table_name=TG_TABLE_NAME;
  ELSE
    INSERT INTO work_cache_revision_scopes(table_name,property_id,revision)
      SELECT table_name,scoped_property,1 FROM work_cache_revision_baselines WHERE table_name=TG_TABLE_NAME
      ON CONFLICT(table_name,property_id) DO UPDATE
        SET revision=work_cache_revision_scopes.revision+1;
  END IF;
  RETURN NULL;
END $$;

CREATE OR REPLACE FUNCTION ensure_work_cache_coverage() RETURNS void LANGUAGE plpgsql AS $$
DECLARE t record;
BEGIN
  FOR t IN SELECT tablename FROM pg_tables WHERE schemaname='public' AND work_cache_tracks(tablename) ORDER BY tablename LOOP
    INSERT INTO work_cache_revision_baselines(table_name) VALUES(t.tablename) ON CONFLICT DO NOTHING;
    IF NOT EXISTS(SELECT 1 FROM pg_trigger WHERE tgrelid=to_regclass(format('public.%I',t.tablename)) AND tgname='work_cache_change') THEN
      EXECUTE format('CREATE TRIGGER work_cache_change AFTER INSERT OR UPDATE OR DELETE ON public.%I FOR EACH ROW EXECUTE FUNCTION work_cache_changed()',t.tablename);
      UPDATE work_cache_revision_baselines SET revision=revision+1 WHERE table_name=t.tablename;
    END IF;
    IF NOT EXISTS(SELECT 1 FROM pg_trigger WHERE tgrelid=to_regclass(format('public.%I',t.tablename)) AND tgname='work_cache_truncate') THEN
      EXECUTE format('CREATE TRIGGER work_cache_truncate AFTER TRUNCATE ON public.%I FOR EACH STATEMENT EXECUTE FUNCTION work_cache_changed()',t.tablename);
      UPDATE work_cache_revision_baselines SET revision=revision+1 WHERE table_name=t.tablename;
    END IF;
  END LOOP;
END $$;
SELECT ensure_work_cache_coverage();
