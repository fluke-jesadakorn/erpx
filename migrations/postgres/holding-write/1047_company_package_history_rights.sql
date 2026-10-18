-- Retain an already effective purchase across renewal/inactivation. No grants,
-- assignments, rollout or billing rows are created or changed here.
ALTER TABLE company_package_grant_events ADD CONSTRAINT company_package_event_scope
  UNIQUE(company_id,package_id,id);
CREATE TABLE company_package_history_rights (
  company_id text NOT NULL,
  package_id text NOT NULL,
  package_version integer NOT NULL CHECK(package_version>0),
  source_event_id text NOT NULL CHECK(length(source_event_id)>0),
  source_image text NOT NULL CHECK(source_image IN ('before','after')),
  source_grant_version integer NOT NULL CHECK(source_grant_version>0),
  retained_at bigint NOT NULL CHECK(retained_at>=0 AND retained_at<=9007199254740991),
  PRIMARY KEY(company_id,package_id,package_version),
  FOREIGN KEY(company_id,package_id,source_event_id)
    REFERENCES company_package_grant_events(company_id,package_id,id)
);
CREATE TRIGGER company_package_history_rights_immutable BEFORE UPDATE OR DELETE
  ON company_package_history_rights FOR EACH ROW EXECUTE FUNCTION commercial_package_audit_immutable();

CREATE FUNCTION commercial_package_history_image(image jsonb) RETURNS jsonb LANGUAGE sql IMMUTABLE AS $$
  SELECT CASE WHEN image IS NULL OR image='null'::jsonb THEN NULL ELSE jsonb_build_object(
    'companyId',image->'companyId','packageId',image->'packageId','packageVersion',image->'packageVersion',
    'state',image->'state','startsAt',image->'startsAt','endsAt',image->'endsAt','version',image->'version') END;
$$;

-- Existing audited, started periods supply history without rewriting a grant.
-- A scheduled period replaced before its start supplies no retained right.
WITH periods AS (
  SELECT e.*,lead(created_at) OVER(PARTITION BY company_id,package_id ORDER BY (after_json->>'version')::integer) replaced_at
  FROM company_package_grant_events e
), eligible AS (
  SELECT *,greatest(created_at,(after_json->>'startsAt')::bigint) became_effective
  FROM periods WHERE after_json->>'state' IN ('active','draining')
    AND created_at<=floor(extract(epoch FROM clock_timestamp())*1000)::bigint
    AND (after_json->>'startsAt')::bigint<=floor(extract(epoch FROM clock_timestamp())*1000)::bigint
    AND ((after_json->>'startsAt')::bigint<=created_at OR replaced_at>(after_json->>'startsAt')::bigint OR replaced_at IS NULL)
)
INSERT INTO company_package_history_rights(company_id,package_id,package_version,source_event_id,source_image,source_grant_version,retained_at)
  SELECT DISTINCT ON(company_id,package_id,(after_json->>'packageVersion')::integer)
    company_id,package_id,(after_json->>'packageVersion')::integer,id,'after',(after_json->>'version')::integer,became_effective
  FROM eligible ORDER BY company_id,package_id,(after_json->>'packageVersion')::integer,became_effective,id;

CREATE FUNCTION commercial_retain_package_history(image jsonb,event_id text,image_kind text,observed_at bigint)
  RETURNS void LANGUAGE plpgsql AS $$
DECLARE audited jsonb;
BEGIN
  IF image IS NULL OR image='null'::jsonb THEN RETURN; END IF;
  SELECT commercial_package_history_image(CASE image_kind WHEN 'before' THEN before_json WHEN 'after' THEN after_json END)
    INTO audited FROM company_package_grant_events WHERE id=event_id;
  IF audited IS NULL OR audited<>image THEN
    RAISE EXCEPTION 'PACKAGE_HISTORY_SOURCE_REQUIRED' USING ERRCODE='23514';
  END IF;
  IF image->>'state' NOT IN ('active','draining') OR (image->>'startsAt')::bigint>observed_at THEN RETURN; END IF;
  INSERT INTO company_package_history_rights(company_id,package_id,package_version,source_event_id,source_image,source_grant_version,retained_at)
    VALUES(image->>'companyId',image->>'packageId',(image->>'packageVersion')::integer,event_id,image_kind,(image->>'version')::integer,observed_at)
    ON CONFLICT DO NOTHING;
END;
$$;

-- Keep the original committed image and final image for each package in this
-- transaction. Intermediate future periods cannot become history merely
-- because COMMIT happens after their starts. Transaction-local state is reset
-- automatically on COMMIT/ROLLBACK and never leaves the Holding connection.
CREATE FUNCTION commercial_package_history_observation() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE
  pending jsonb:=coalesce(nullif(current_setting('erpx.commercial_history_pending',true),'')::jsonb,'{}'::jsonb);
  package_key text:=jsonb_build_array(NEW.company_id,NEW.package_id)::text;
  entry jsonb;
  current_image jsonb;
  instant bigint:=floor(extract(epoch FROM clock_timestamp())*1000)::bigint;
BEGIN
  IF TG_ARGV[0]='observe' THEN
    SELECT jsonb_build_object('companyId',company_id,'packageId',package_id,'packageVersion',package_version,
      'state',state,'startsAt',starts_at,'endsAt',ends_at,'version',version)
      INTO current_image FROM company_package_grants WHERE company_id=NEW.company_id AND package_id=NEW.package_id;
    IF current_image IS NULL OR current_image<>commercial_package_history_image(NEW.after_json)
       OR NEW.after_json->>'companyId'<>NEW.company_id OR NEW.after_json->>'packageId'<>NEW.package_id THEN
      RAISE EXCEPTION 'PACKAGE_HISTORY_CURRENT_AUDIT_REQUIRED' USING ERRCODE='23514';
    END IF;
    entry:=coalesce(pending->package_key,jsonb_build_object('before',commercial_package_history_image(NEW.before_json),'beforeEvent',NEW.id));
    entry:=entry||jsonb_build_object('after',commercial_package_history_image(NEW.after_json),'afterEvent',NEW.id);
    PERFORM set_config('erpx.commercial_history_pending',jsonb_set(pending,ARRAY[package_key],entry)::text,true);
  ELSE
    entry:=pending->package_key;
    IF entry IS NULL THEN RETURN NEW; END IF;
    PERFORM commercial_retain_package_history(entry->'before',entry->>'beforeEvent','before',instant);
    PERFORM commercial_retain_package_history(entry->'after',entry->>'afterEvent','after',instant);
    PERFORM set_config('erpx.commercial_history_pending',(pending-package_key)::text,true);
  END IF;
  RETURN NEW;
END;
$$;
CREATE TRIGGER company_package_history_observe AFTER INSERT ON company_package_grant_events
  FOR EACH ROW EXECUTE FUNCTION commercial_package_history_observation('observe');
CREATE CONSTRAINT TRIGGER company_package_history_commit AFTER INSERT ON company_package_grant_events
  DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION commercial_package_history_observation('commit');

CREATE FUNCTION commercial_package_current_audit_required() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE image jsonb;
BEGIN
  SELECT jsonb_build_object('companyId',company_id,'packageId',package_id,'packageVersion',package_version,
    'state',state,'startsAt',starts_at,'endsAt',ends_at,'version',version,'reason',reason,'evidence',evidence) INTO image
    FROM company_package_grants WHERE company_id=NEW.company_id AND package_id=NEW.package_id;
  IF image IS NOT NULL AND NOT EXISTS(SELECT 1 FROM company_package_grant_events e
    WHERE e.company_id=NEW.company_id AND e.package_id=NEW.package_id AND e.after_json=image) THEN
    RAISE EXCEPTION 'PACKAGE_HISTORY_CURRENT_AUDIT_REQUIRED' USING ERRCODE='23514';
  END IF;
  RETURN NEW;
END;
$$;
CREATE CONSTRAINT TRIGGER company_package_current_audit_required AFTER INSERT OR UPDATE ON company_package_grants
  DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION commercial_package_current_audit_required();
