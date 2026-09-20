CREATE TABLE organization_brand_profiles (
  scope_kind TEXT NOT NULL CHECK(scope_kind IN ('holding','branch')),
  scope_id TEXT NOT NULL,
  company_id TEXT,
  primary_attachment_id TEXT REFERENCES attachments(id),
  inverse_attachment_id TEXT REFERENCES attachments(id),
  document_mode TEXT NOT NULL CHECK(document_mode IN ('holding','branch','both')),
  updated_by TEXT NOT NULL,
  updated_at BIGINT NOT NULL,
  PRIMARY KEY(scope_kind,scope_id),
  CHECK((scope_kind='holding' AND company_id IS NULL AND document_mode='holding') OR (scope_kind='branch' AND company_id IS NOT NULL))
);

CREATE INDEX organization_brand_company ON organization_brand_profiles(company_id,scope_kind,scope_id);
