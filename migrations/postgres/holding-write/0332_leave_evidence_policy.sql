-- Versioned leave-evidence rules and immutable final-review manifests.
ALTER TABLE leave_policy_rules
  ADD COLUMN evidence_min_units NUMERIC(8,2),
  ADD COLUMN evidence_purpose TEXT CHECK (evidence_purpose IN ('medical_certificate','leave_supporting_photo')),
  ADD COLUMN evidence_min_count INTEGER NOT NULL DEFAULT 0 CHECK (evidence_min_count BETWEEN 0 AND 100),
  ADD COLUMN evidence_due_stage TEXT NOT NULL DEFAULT 'before_final_approval'
    CHECK (evidence_due_stage IN ('before_submit','before_final_approval')),
  ADD COLUMN evidence_grace_days INTEGER NOT NULL DEFAULT 0 CHECK (evidence_grace_days BETWEEN 0 AND 365),
  ADD COLUMN evidence_note TEXT NOT NULL DEFAULT '',
  ADD COLUMN evidence_privacy TEXT NOT NULL DEFAULT 'requester_approvers_hr'
    CHECK (evidence_privacy = 'requester_approvers_hr'),
  ADD CONSTRAINT leave_policy_rule_evidence_complete CHECK (
    (evidence_purpose IS NULL AND evidence_min_count = 0 AND evidence_min_units IS NULL)
    OR
    (evidence_purpose IS NOT NULL AND evidence_min_count > 0 AND evidence_min_units IS NOT NULL AND evidence_min_units > 0)
  );

-- Preserve the old type-level threshold as an explicit rule on every existing
-- policy version. Sick/medical codes become medical-certificate requirements;
-- other historical rules retain generic supporting-photo semantics.
UPDATE leave_policy_rules AS rule
SET evidence_min_units = type.evidence_after_units,
    evidence_purpose = CASE
      WHEN lower(type.code) ~ '(sick|medical|illness)' THEN 'medical_certificate'
      ELSE 'leave_supporting_photo'
    END,
    evidence_min_count = 1,
    evidence_due_stage = 'before_final_approval',
    evidence_grace_days = 0,
    evidence_note = type.evidence_note
FROM leave_types AS type
WHERE type.id = rule.leave_type_id
  AND type.evidence_after_units IS NOT NULL;

ALTER TABLE leave_requests
  ADD COLUMN evidence_policy_snapshot JSONB,
  ADD COLUMN evidence_due_at BIGINT,
  ADD COLUMN evidence_version INTEGER NOT NULL DEFAULT 0 CHECK (evidence_version >= 0),
  ADD COLUMN submitted_at BIGINT;

CREATE TABLE leave_evidence_manifests (
  id TEXT PRIMARY KEY,
  request_id TEXT NOT NULL REFERENCES leave_requests(id),
  company_id TEXT NOT NULL,
  approval_instance_id TEXT REFERENCES approval_instances(id),
  decision_command_id TEXT NOT NULL UNIQUE REFERENCES command_receipts(command_id),
  request_version INTEGER NOT NULL CHECK (request_version > 0),
  policy_version_id TEXT REFERENCES leave_policy_versions(id),
  reviewer_user_id TEXT NOT NULL,
  manifest_json JSONB NOT NULL,
  created_at BIGINT NOT NULL,
  UNIQUE (request_id, request_version)
);
CREATE INDEX leave_evidence_manifest_request
  ON leave_evidence_manifests(company_id, request_id, created_at DESC);

CREATE FUNCTION leave_evidence_manifest_immutable() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  RAISE EXCEPTION 'Leave evidence manifests are append-only';
END $$;
CREATE TRIGGER leave_evidence_manifest_immutable
  BEFORE UPDATE OR DELETE ON leave_evidence_manifests
  FOR EACH ROW EXECUTE FUNCTION leave_evidence_manifest_immutable();

INSERT INTO iam_permissions(id,definition) VALUES
  ('leave.evidence.read',jsonb_build_object(
    'id','leave.evidence.read',
    'label','Read restricted leave evidence',
    'module','people_payroll',
    'namespace','customer',
    'availability','live',
    'risk','privileged',
    'roles','["holding_owner","hr_admin"]'::jsonb,
    'conditions','["Requester, current eligible approver, recorded decision maker, or authorized HR only"]'::jsonb,
    'permittedScopes','["holding","branch","department"]'::jsonb,
    'identityBased',false
  ))
ON CONFLICT(id) DO UPDATE SET definition=excluded.definition;

INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at)
SELECT r.company_id,r.id,r.published_revision+1,
  v.permissions_json || CASE WHEN v.permissions_json ? 'leave.evidence.read' THEN '[]'::jsonb ELSE '["leave.evidence.read"]'::jsonb END,
  v.grantable_json || CASE WHEN v.grantable_json ? 'leave.evidence.read' THEN '[]'::jsonb ELSE '["leave.evidence.read"]'::jsonb END,
  v.scopes_json,'system:leave-evidence',floor(extract(epoch FROM clock_timestamp())*1000)::bigint
FROM iam_roles r
JOIN iam_role_revisions v ON v.company_id=r.company_id AND v.role_id=r.id AND v.revision=r.published_revision
WHERE r.company_id='*' AND r.id IN ('holding_owner','hr_admin')
  AND (NOT v.permissions_json ? 'leave.evidence.read' OR NOT v.grantable_json ? 'leave.evidence.read');
UPDATE iam_roles AS role
SET published_revision=(
  SELECT max(revision) FROM iam_role_revisions AS revision
  WHERE revision.company_id=role.company_id AND revision.role_id=role.id
)
WHERE role.company_id='*' AND role.id IN ('holding_owner','hr_admin');

DO $$
DECLARE migration_role TEXT := current_user; app_role TEXT; jobs_role TEXT;
BEGIN
  IF migration_role !~ '_migration$' THEN RETURN; END IF;
  app_role := regexp_replace(migration_role, '_migration$', '_app');
  jobs_role := regexp_replace(migration_role, '_migration$', '_jobs');
  IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = app_role)
    OR NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = jobs_role) THEN
    RAISE EXCEPTION 'ERPX_RUNTIME_ROLES_MISSING';
  END IF;
  EXECUTE format('GRANT SELECT,INSERT ON TABLE leave_evidence_manifests TO %I,%I', app_role, jobs_role);
END $$;
