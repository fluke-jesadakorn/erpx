-- Additive: setup attestations and historical capability evidence remain intact.
CREATE TABLE company_release_revisions (
  company_id TEXT PRIMARY KEY,
  revision BIGINT NOT NULL DEFAULT 0 CHECK(revision >= 0)
);
INSERT INTO company_release_revisions(company_id)
  SELECT id FROM organization_nodes WHERE kind='company' ON CONFLICT DO NOTHING;

CREATE TABLE company_release_acceptances (
  id TEXT PRIMARY KEY,
  company_id TEXT NOT NULL REFERENCES organization_nodes(id),
  scope_version TEXT NOT NULL,
  software_version TEXT NOT NULL,
  configuration_revision BIGINT NOT NULL,
  setup_version BIGINT NOT NULL,
  evidence_sha256 TEXT NOT NULL CHECK(evidence_sha256 ~ '^[a-f0-9]{64}$'),
  evidence_json TEXT NOT NULL CHECK(erpx_json_valid(evidence_json)),
  requested_by TEXT NOT NULL,
  reason TEXT NOT NULL,
  version BIGINT NOT NULL DEFAULT 1 CHECK(version > 0),
  revoked_at BIGINT,
  created_at BIGINT NOT NULL,
  updated_at BIGINT NOT NULL,
  UNIQUE(company_id,id)
);
CREATE INDEX company_release_acceptances_current ON company_release_acceptances(company_id,created_at DESC,id);

CREATE TABLE company_release_gate_reviews (
  company_id TEXT NOT NULL,
  release_id TEXT NOT NULL,
  criterion_id TEXT NOT NULL,
  state TEXT NOT NULL CHECK(state IN ('pending','passed','not_applicable')),
  reviewer_id TEXT NOT NULL,
  assignment_id TEXT NOT NULL,
  permission TEXT NOT NULL,
  reason TEXT NOT NULL,
  reviewed_at BIGINT NOT NULL,
  PRIMARY KEY(release_id,criterion_id),
  FOREIGN KEY(company_id,release_id) REFERENCES company_release_acceptances(company_id,id)
);
CREATE TABLE company_release_events (
  id TEXT PRIMARY KEY,
  company_id TEXT NOT NULL,
  release_id TEXT NOT NULL,
  version BIGINT NOT NULL,
  action TEXT NOT NULL CHECK(action IN ('submitted','reviewed','revoked')),
  actor_id TEXT NOT NULL,
  detail_json TEXT NOT NULL CHECK(erpx_json_valid(detail_json)),
  created_at BIGINT NOT NULL,
  UNIQUE(release_id,version),
  FOREIGN KEY(company_id,release_id) REFERENCES company_release_acceptances(company_id,id)
);
CREATE FUNCTION release_evidence_immutable() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN RAISE EXCEPTION 'RELEASE_EVIDENCE_IMMUTABLE'; END $$;
CREATE TRIGGER release_events_immutable BEFORE UPDATE OR DELETE ON company_release_events
  FOR EACH ROW EXECUTE FUNCTION release_evidence_immutable();
CREATE FUNCTION release_snapshot_guard() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF TG_OP='DELETE' OR (to_jsonb(NEW)-ARRAY['version','revoked_at','updated_at']) IS DISTINCT FROM
    (to_jsonb(OLD)-ARRAY['version','revoked_at','updated_at']) THEN
    RAISE EXCEPTION 'RELEASE_SNAPSHOT_IMMUTABLE';
  END IF;
  RETURN NEW;
END $$;
CREATE TRIGGER release_snapshot_immutable BEFORE UPDATE OR DELETE ON company_release_acceptances
  FOR EACH ROW EXECUTE FUNCTION release_snapshot_guard();

-- Settings changes serialize with a release review/activation. Operational data
-- (sales, guests, payroll runs, business dates, room cleaning) do not invalidate it.
CREATE FUNCTION invalidate_company_release() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE before_data JSONB; after_data JSONB; company TEXT; property TEXT;
BEGIN
  before_data := CASE WHEN TG_OP='INSERT' THEN '{}'::jsonb ELSE to_jsonb(OLD) END;
  after_data := CASE WHEN TG_OP='DELETE' THEN '{}'::jsonb ELSE to_jsonb(NEW) END;
  IF TG_TABLE_NAME='pms_records' THEN
    IF COALESCE(after_data->>'kind',before_data->>'kind') NOT IN ('room_type','room','rate') THEN RETURN NULL; END IF;
    -- Room operational status changes happen during every stay.
    IF COALESCE(after_data->>'kind',before_data->>'kind')='room' AND TG_OP='UPDATE' THEN
      IF ((after_data->>'data_json')::jsonb-ARRAY['status','housekeeping','cleaningStatus','inspectionStatus']) =
         ((before_data->>'data_json')::jsonb-ARRAY['status','housekeeping','cleaningStatus','inspectionStatus']) THEN RETURN NULL; END IF;
    END IF;
    property := COALESCE(after_data->>'property_id',before_data->>'property_id');
    SELECT company_id INTO company FROM pms_properties WHERE id=property;
  ELSIF TG_TABLE_NAME='pms_properties' THEN
    IF TG_OP='UPDATE' AND (after_data-ARRAY['version','updated_at','business_date','state']) =
      (before_data-ARRAY['version','updated_at','business_date','state']) THEN RETURN NULL; END IF;
    company := COALESCE(after_data->>'company_id',before_data->>'company_id');
  ELSIF TG_TABLE_NAME='hr_country_packs' THEN
    UPDATE company_release_revisions SET revision=revision+1;
    RETURN NULL;
  ELSE
    IF TG_OP='UPDATE' AND after_data=before_data THEN RETURN NULL; END IF;
    company := COALESCE(after_data->>'company_id',before_data->>'company_id');
  END IF;
  IF company IS NOT NULL THEN
    INSERT INTO company_release_revisions(company_id,revision) VALUES(company,1)
      ON CONFLICT(company_id) DO UPDATE SET revision=company_release_revisions.revision+1;
  END IF;
  RETURN NULL;
END $$;
DO $$ DECLARE tab TEXT; BEGIN
  FOREACH tab IN ARRAY ARRAY['organization_nodes','company_business_acceptance',
    'finance_charts','finance_accounts','commerce_settings','pos_settings','pos_registers',
    'pos_catalog','hr_pay_groups','hr_country_packs','pms_properties','pms_records'] LOOP
    EXECUTE format('CREATE TRIGGER release_configuration_changed AFTER INSERT OR UPDATE OR DELETE ON %I FOR EACH ROW EXECUTE FUNCTION invalidate_company_release()',tab);
  END LOOP;
END $$;
