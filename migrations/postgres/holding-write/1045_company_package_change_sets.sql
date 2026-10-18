-- Reviewed, atomic per-Company package changes. No grants, assignments or rollout changes.
CREATE TABLE company_package_revisions (
  company_id text PRIMARY KEY,
  version bigint NOT NULL DEFAULT 0 CHECK(version>=0)
);
INSERT INTO company_package_revisions(company_id,version)
  SELECT company_id,sum(version) FROM company_package_grants GROUP BY company_id;
CREATE FUNCTION commercial_package_revision_changed() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  INSERT INTO company_package_revisions(company_id,version) VALUES(NEW.company_id,1)
    ON CONFLICT(company_id) DO UPDATE SET version=company_package_revisions.version+1;
  RETURN NEW;
END;
$$;
CREATE TRIGGER company_package_revision_changed AFTER INSERT OR UPDATE ON company_package_grants
  FOR EACH ROW EXECUTE FUNCTION commercial_package_revision_changed();
CREATE TABLE company_package_change_sets (
  id text PRIMARY KEY,
  company_id text NOT NULL,
  actor_id text NOT NULL,
  request_hash text NOT NULL CHECK(request_hash ~ '^[a-f0-9]{64}$'),
  result_json jsonb NOT NULL,
  created_at bigint NOT NULL
);
CREATE INDEX company_package_change_sets_company ON company_package_change_sets(company_id,created_at,id);
CREATE FUNCTION commercial_package_audit_immutable() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  RAISE EXCEPTION 'COMMERCIAL_PACKAGE_AUDIT_IMMUTABLE' USING ERRCODE='55000';
END;
$$;
CREATE TRIGGER company_package_change_sets_immutable BEFORE UPDATE OR DELETE ON company_package_change_sets
  FOR EACH ROW EXECUTE FUNCTION commercial_package_audit_immutable();
CREATE TRIGGER company_package_grant_events_immutable BEFORE UPDATE OR DELETE ON company_package_grant_events
  FOR EACH ROW EXECUTE FUNCTION commercial_package_audit_immutable();
