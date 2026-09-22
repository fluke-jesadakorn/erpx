-- Transactional customer merge lineage and reviewed source repointing.
ALTER TABLE finance_parties ADD COLUMN merged_into_id TEXT REFERENCES finance_parties(id);
ALTER TABLE finance_parties ADD COLUMN merged_at BIGINT;
ALTER TABLE finance_parties ADD COLUMN merged_by TEXT;
CREATE INDEX finance_party_merge_lookup ON finance_parties(company_id,merged_into_id);

ALTER TABLE customer_duplicate_events DROP CONSTRAINT IF EXISTS customer_duplicate_events_action_check;
ALTER TABLE customer_duplicate_events ADD CONSTRAINT customer_duplicate_events_action_check CHECK(action IN ('request','distinct','duplicate','reopen','merge'));

ALTER TABLE customer_duplicate_reviews DROP CONSTRAINT IF EXISTS customer_duplicate_reviews_status_check;
ALTER TABLE customer_duplicate_reviews DROP CONSTRAINT IF EXISTS customer_duplicate_reviews_check;
ALTER TABLE customer_duplicate_reviews DROP CONSTRAINT IF EXISTS customer_duplicate_reviews_check2;
ALTER TABLE customer_duplicate_reviews ADD CONSTRAINT customer_duplicate_reviews_status_check CHECK(status IN ('pending','distinct','duplicate','merged'));
ALTER TABLE customer_duplicate_reviews ADD CONSTRAINT customer_duplicate_reviews_check CHECK((status IN ('duplicate','merged'))=(canonical_customer_id IS NOT NULL));
