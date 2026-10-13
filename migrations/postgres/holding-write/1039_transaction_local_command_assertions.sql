-- Transaction-local command guards; preserve legacy rows and the SQL interface.
-- https://www.postgresql.org/docs/18/functions-admin.html
-- https://www.postgresql.org/docs/18/sql-set.html
ALTER TABLE command_assertions RENAME TO command_assertion_baselines;
CREATE VIEW command_assertions WITH (security_invoker=true) AS
SELECT command_id,step,ok FROM command_assertion_baselines
UNION ALL
SELECT commands.key AS command_id,steps.key AS step,(steps.value #>> '{}')::bigint AS ok
FROM jsonb_each(COALESCE(NULLIF(current_setting('erpx.command_assertions',true),''),'{}')::jsonb) commands
CROSS JOIN LATERAL jsonb_each(commands.value) steps;

CREATE FUNCTION command_assertions_changed() RETURNS trigger
LANGUAGE plpgsql SECURITY INVOKER SET search_path=pg_catalog,public AS $$
DECLARE
  guards jsonb := COALESCE(NULLIF(current_setting('erpx.command_assertions',true),''),'{}')::jsonb;
  command_guards jsonb;
  local_old boolean := false;
BEGIN
  IF jsonb_typeof(guards)<>'object' THEN
    RAISE EXCEPTION 'Invalid transaction command assertions' USING ERRCODE='22023';
  END IF;
  IF TG_OP IN ('DELETE','UPDATE') THEN
    local_old := guards ? OLD.command_id AND COALESCE(guards->OLD.command_id,'{}') ? OLD.step;
    IF local_old THEN
      command_guards := (guards->OLD.command_id)-OLD.step;
      guards := CASE WHEN command_guards='{}'::jsonb THEN guards-OLD.command_id
                     ELSE guards || jsonb_build_object(OLD.command_id,command_guards) END;
    ELSE
      -- Retain ordinary administrative behavior for every pre-migration row.
      IF TG_OP='DELETE' THEN
        DELETE FROM public.command_assertion_baselines WHERE command_id=OLD.command_id AND step=OLD.step;
        RETURN OLD;
      END IF;
      IF COALESCE(guards->NEW.command_id,'{}') ? NEW.step THEN
        RAISE EXCEPTION 'duplicate key value violates unique constraint "command_assertions_pkey"'
          USING ERRCODE='23505',CONSTRAINT='command_assertions_pkey',TABLE='command_assertions';
      END IF;
      UPDATE public.command_assertion_baselines SET command_id=NEW.command_id,step=NEW.step,ok=NEW.ok
        WHERE command_id=OLD.command_id AND step=OLD.step;
      RETURN NEW;
    END IF;
    IF TG_OP='DELETE' THEN
      PERFORM set_config('erpx.command_assertions',guards::text,true);
      RETURN OLD;
    END IF;
  END IF;
  IF NEW.command_id IS NULL OR NEW.step IS NULL OR NEW.ok IS NULL THEN
    RAISE EXCEPTION 'null value in command assertion violates not-null constraint'
      USING ERRCODE='23502',TABLE='command_assertions',COLUMN=CASE WHEN NEW.command_id IS NULL THEN 'command_id' WHEN NEW.step IS NULL THEN 'step' ELSE 'ok' END;
  END IF;
  IF NEW.ok<>1 THEN
    RAISE EXCEPTION 'new row for relation "command_assertions" violates check constraint "command_assertions_ok_check"'
      USING ERRCODE='23514',CONSTRAINT='command_assertions_ok_check',TABLE='command_assertions',
        DETAIL=format('Failing row contains (%s, %s, %s).',NEW.command_id,NEW.step,NEW.ok);
  END IF;
  command_guards := COALESCE(guards->NEW.command_id,'{}');
  IF jsonb_typeof(command_guards)<>'object' THEN
    RAISE EXCEPTION 'Invalid transaction command assertions' USING ERRCODE='22023';
  END IF;
  IF command_guards ? NEW.step OR EXISTS(SELECT 1 FROM public.command_assertion_baselines WHERE command_id=NEW.command_id AND step=NEW.step) THEN
    RAISE EXCEPTION 'duplicate key value violates unique constraint "command_assertions_pkey"'
      USING ERRCODE='23505',CONSTRAINT='command_assertions_pkey',TABLE='command_assertions';
  END IF;
  guards := guards || jsonb_build_object(NEW.command_id,command_guards || jsonb_build_object(NEW.step,NEW.ok));
  PERFORM set_config('erpx.command_assertions',guards::text,true);
  RETURN NEW;
END $$;
CREATE TRIGGER command_assertions_change INSTEAD OF INSERT OR UPDATE OR DELETE
ON command_assertions FOR EACH ROW EXECUTE FUNCTION command_assertions_changed();

-- Preserved legacy assertion rows are transient workflow guards, not cached facts.
CREATE OR REPLACE FUNCTION work_cache_tracks(name text) RETURNS boolean LANGUAGE sql IMMUTABLE AS $$
  SELECT name NOT IN ('work_cache_revisions','work_cache_revision_baselines','work_cache_revision_scopes',
    'erpx_migrations','outbox_events','command_assertions','command_assertion_baselines','command_receipts','audit_events','runtime_health',
    'iam_audit','iam_receipts','iam_sync_jobs','um_history','seed_repairs',
    'home_read_cursors','active_work_contexts','work_role_preferences',
    'work_card_preferences','navigation_preferences','pos_audit','pms_jobs',
    'legal_access_events','legal_document_inspections','legal_ai_proposals')
    AND name NOT LIKE '%\_receipts' ESCAPE '\'
$$;
SELECT ensure_work_cache_coverage();
