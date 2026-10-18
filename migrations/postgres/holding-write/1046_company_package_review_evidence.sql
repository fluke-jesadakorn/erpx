CREATE TABLE company_package_review_evidence (
  change_set_id text PRIMARY KEY REFERENCES company_package_change_sets(id) DEFERRABLE INITIALLY DEFERRED,
  company_id text NOT NULL,
  actor_id text NOT NULL,
  assignment_id text NOT NULL,
  role_id text NOT NULL,
  role_revision integer NOT NULL CHECK(role_revision>0),
  authority_revision text NOT NULL,
  catalogue_hash text NOT NULL,
  review_hash text NOT NULL,
  reviewed_at bigint NOT NULL,
  applied_at bigint NOT NULL,
  comparison_json jsonb NOT NULL
);
CREATE TRIGGER company_package_review_evidence_immutable BEFORE UPDATE OR DELETE ON company_package_review_evidence
  FOR EACH ROW EXECUTE FUNCTION commercial_package_audit_immutable();
