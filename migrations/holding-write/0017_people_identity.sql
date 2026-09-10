ALTER TABLE employees ADD COLUMN person_id TEXT REFERENCES hr_people(id);
CREATE UNIQUE INDEX employee_person_company_idx ON employees(person_id,company_id) WHERE person_id IS NOT NULL;
ALTER TABLE hr_people ADD COLUMN merged_into_id TEXT REFERENCES hr_people(id);
CREATE TABLE seed_repairs (
  repair_id TEXT PRIMARY KEY,
  snapshot_json TEXT NOT NULL CHECK(json_valid(snapshot_json)),
  created_at INTEGER NOT NULL
) STRICT;
