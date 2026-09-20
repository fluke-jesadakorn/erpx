CREATE TABLE legal_enrollment_attempts(share_id text PRIMARY KEY REFERENCES legal_shares(id),window_start bigint NOT NULL,attempts integer NOT NULL CHECK(attempts>0));
INSERT INTO work_cache_revisions(table_name) VALUES('legal_enrollment_attempts');
CREATE TRIGGER work_cache_change AFTER INSERT OR UPDATE OR DELETE ON legal_enrollment_attempts FOR EACH ROW EXECUTE FUNCTION work_cache_changed();
CREATE TRIGGER work_cache_truncate AFTER TRUNCATE ON legal_enrollment_attempts FOR EACH STATEMENT EXECUTE FUNCTION work_cache_changed();
