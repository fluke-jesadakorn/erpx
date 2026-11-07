-- POS posting receipts and the Finance journal they reference may be written in
-- either order inside one serializable owner transaction: the native POS command
-- retains its posting receipt first, then the Finance owner posts the journal on
-- the same connection. The reference remains enforced, now at commit.
ALTER TABLE pos_postings DROP CONSTRAINT pos_postings_journal_id_fkey;
ALTER TABLE pos_postings ADD CONSTRAINT pos_postings_journal_id_fkey
  FOREIGN KEY (journal_id) REFERENCES finance_journals(id) DEFERRABLE INITIALLY DEFERRED;
