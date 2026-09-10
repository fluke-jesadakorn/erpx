PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS workspace_records (
  module_id TEXT NOT NULL,
  id TEXT NOT NULL,
  section_id TEXT NOT NULL,
  title TEXT NOT NULL CHECK(length(title) BETWEEN 1 AND 160),
  description TEXT NOT NULL DEFAULT '' CHECK(length(description) <= 2000),
  status TEXT NOT NULL DEFAULT 'draft'
    CHECK(status IN ('draft','in_review','approved','completed','rejected')),
  fields_json TEXT NOT NULL CHECK(json_valid(fields_json)),
  company_id TEXT NOT NULL,
  branch_id TEXT,
  department_id TEXT,
  version INTEGER NOT NULL DEFAULT 1 CHECK(version > 0),
  created_by TEXT NOT NULL,
  updated_by TEXT NOT NULL,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  PRIMARY KEY(module_id, id)
) STRICT;
CREATE INDEX IF NOT EXISTS workspace_records_scope_idx
  ON workspace_records(company_id, branch_id, department_id, module_id, section_id, updated_at DESC);

CREATE TABLE IF NOT EXISTS workspace_record_transitions (
  id TEXT PRIMARY KEY,
  module_id TEXT NOT NULL,
  record_id TEXT NOT NULL,
  from_status TEXT NOT NULL,
  to_status TEXT NOT NULL,
  action TEXT NOT NULL,
  note TEXT NOT NULL DEFAULT '',
  actor_id TEXT NOT NULL,
  assignment_id TEXT NOT NULL,
  record_version INTEGER NOT NULL,
  created_at INTEGER NOT NULL,
  FOREIGN KEY(module_id, record_id) REFERENCES workspace_records(module_id, id)
) STRICT;
CREATE INDEX IF NOT EXISTS workspace_record_transitions_idx
  ON workspace_record_transitions(module_id, record_id, created_at DESC);

CREATE TABLE IF NOT EXISTS record_reference_edges (
  host_module TEXT NOT NULL,
  host_id TEXT NOT NULL,
  target_module TEXT NOT NULL,
  target_id TEXT NOT NULL,
  kind TEXT NOT NULL CHECK(kind IN ('dependsOn','blocks','billedBy','fulfills','authoredWith')),
  ordinal INTEGER NOT NULL CHECK(ordinal BETWEEN 0 AND 49),
  created_by TEXT NOT NULL,
  created_at INTEGER NOT NULL,
  PRIMARY KEY(host_module, host_id, target_module, target_id),
  FOREIGN KEY(host_module, host_id) REFERENCES workspace_records(module_id, id),
  FOREIGN KEY(target_module, target_id) REFERENCES workspace_records(module_id, id)
) STRICT;
CREATE INDEX IF NOT EXISTS record_reference_backlinks_idx
  ON record_reference_edges(target_module, target_id, created_at DESC);

CREATE TABLE IF NOT EXISTS record_field_suggestions (
  id TEXT PRIMARY KEY,
  module_id TEXT NOT NULL,
  record_id TEXT NOT NULL,
  field_key TEXT NOT NULL,
  proposed_value_json TEXT NOT NULL CHECK(json_valid(proposed_value_json)),
  previous_value_digest TEXT NOT NULL,
  source_versions_json TEXT NOT NULL CHECK(json_valid(source_versions_json)),
  provenance_json TEXT NOT NULL CHECK(json_valid(provenance_json)),
  state TEXT NOT NULL CHECK(state IN ('ready','applied','rejected','conflicted')),
  created_by TEXT NOT NULL,
  applied_by TEXT,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  FOREIGN KEY(module_id, record_id) REFERENCES workspace_records(module_id, id)
) STRICT;
CREATE INDEX IF NOT EXISTS record_field_suggestions_idx
  ON record_field_suggestions(module_id, record_id, state, created_at DESC);
