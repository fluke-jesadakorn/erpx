-- Preserve the authority and receipt versions actually reviewed. Legacy
-- decisions remain intact; their existing audit and routing history is retained.
ALTER TABLE approval_decisions ADD COLUMN authority_json TEXT
  CHECK(authority_json IS NULL OR json_valid(authority_json));
