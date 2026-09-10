CREATE TABLE work_card_preferences (
  company_id TEXT NOT NULL REFERENCES organization_nodes(id),
  user_id TEXT NOT NULL,
  pinned_ids_json TEXT CHECK(pinned_ids_json IS NULL OR (json_valid(pinned_ids_json) AND json_type(pinned_ids_json)='array' AND json_array_length(pinned_ids_json)<=6)),
  version INTEGER NOT NULL CHECK(version > 0),
  updated_at INTEGER NOT NULL,
  PRIMARY KEY(company_id,user_id)
) STRICT;
