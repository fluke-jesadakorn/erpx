-- Retain the original migration and preserve the year of every imported balance.
ALTER TABLE leave_balance_entries ADD COLUMN entitlement_year INTEGER;
ALTER TABLE leave_balance_entries DISABLE TRIGGER leave_balance_entries_immutable;
UPDATE leave_balance_entries SET entitlement_year=left(effective_date,4)::int;
ALTER TABLE leave_balance_entries ENABLE TRIGGER leave_balance_entries_immutable;
ALTER TABLE leave_balance_entries ALTER COLUMN entitlement_year SET NOT NULL;
CREATE FUNCTION leave_entry_year() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN
 IF NEW.entitlement_year IS NULL THEN NEW.entitlement_year:=left(NEW.effective_date,4)::int; END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER leave_entry_year BEFORE INSERT ON leave_balance_entries FOR EACH ROW EXECUTE FUNCTION leave_entry_year();
ALTER TABLE leave_policy_rules ADD COLUMN employee_eligible BOOLEAN NOT NULL DEFAULT true;
ALTER TABLE leave_policy_rules ADD COLUMN contractor_eligible BOOLEAN NOT NULL DEFAULT false;
ALTER TABLE leave_cancellation_reviews DROP CONSTRAINT leave_cancellation_reviews_request_id_state_key;
CREATE UNIQUE INDEX leave_one_pending_cancellation ON leave_cancellation_reviews(request_id) WHERE state='pending_approval';
-- Imported policy assignments are explicit, untracked and never establish an allowance.
INSERT INTO leave_policy_assignments(id,company_id,engagement_id,policy_version_id,effective_from,created_by,created_at)
 SELECT 'leave-legacy-assignment-'||e.id,e.company_id,e.id,p.id,e.start_date,'migration',0
 FROM hr_engagements e JOIN leave_policy_versions p ON p.company_id=e.company_id AND p.policy_key='legacy-untracked'
 WHERE NOT EXISTS(SELECT 1 FROM leave_policy_assignments a WHERE a.engagement_id=e.id AND a.company_id=e.company_id);
UPDATE leave_policy_rules r SET contractor_eligible=true FROM leave_policy_versions p WHERE p.id=r.policy_version_id AND p.policy_key='legacy-untracked';
-- Preserve original year-specific fields before the old table becomes a derived projection.
CREATE TABLE leave_legacy_balance_provenance AS SELECT b.*,e.company_id FROM hr_leave_balances b JOIN hr_engagements e ON e.id=b.engagement_id;
CREATE VIEW leave_balance_projection AS SELECT company_id,engagement_id,leave_type_id,entitlement_year,
 coalesce(sum(units),0) available,
 coalesce(-sum(units) FILTER(WHERE kind IN ('reservation','reservation_release')),0) reserved,
 coalesce(-sum(units) FILTER(WHERE kind IN ('usage','usage_reversal')),0) used,
 coalesce(sum(units) FILTER(WHERE kind='accrual'),0) accrued
 FROM leave_balance_entries GROUP BY company_id,engagement_id,leave_type_id,entitlement_year;
CREATE FUNCTION leave_compatibility_projection() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN
 INSERT INTO hr_leave_balances(id,engagement_id,leave_code,year,opening_days,accrued_days,carried_days,used_days)
 SELECT 'leave-projection-'||NEW.engagement_id||'-'||NEW.leave_type_id||'-'||NEW.entitlement_year,NEW.engagement_id,(SELECT code FROM leave_types WHERE id=NEW.leave_type_id),NEW.entitlement_year,
 coalesce(sum(units) FILTER(WHERE kind IN ('opening','grant','adjustment')),0),
 coalesce(sum(units) FILTER(WHERE kind='accrual'),0),coalesce(sum(units) FILTER(WHERE kind='carryover'),0),
 coalesce(-sum(units) FILTER(WHERE kind IN ('usage','usage_reversal')),0)
 FROM leave_balance_entries WHERE company_id=NEW.company_id AND engagement_id=NEW.engagement_id AND entitlement_year=NEW.entitlement_year AND leave_type_id=NEW.leave_type_id
 ON CONFLICT(engagement_id,leave_code,year) DO UPDATE SET opening_days=excluded.opening_days,accrued_days=excluded.accrued_days,carried_days=excluded.carried_days,used_days=excluded.used_days;
 RETURN NEW;
END $$;
CREATE TRIGGER leave_compatibility_projection AFTER INSERT ON leave_balance_entries FOR EACH ROW EXECUTE FUNCTION leave_compatibility_projection();
-- Reject company mismatches even for direct SQL callers.
CREATE FUNCTION leave_company_integrity() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN
 IF NOT EXISTS(SELECT 1 FROM hr_engagements WHERE id=NEW.engagement_id AND company_id=NEW.company_id) THEN RAISE EXCEPTION 'LEAVE_COMPANY_MISMATCH'; END IF;
 IF TG_TABLE_NAME='leave_policy_assignments' THEN
  IF NOT EXISTS(SELECT 1 FROM leave_policy_versions WHERE id=NEW.policy_version_id AND company_id=NEW.company_id) OR
   (NEW.holiday_calendar_id IS NOT NULL AND NOT EXISTS(SELECT 1 FROM leave_holiday_calendars WHERE id=NEW.holiday_calendar_id AND company_id=NEW.company_id)) THEN RAISE EXCEPTION 'LEAVE_COMPANY_MISMATCH'; END IF;
 ELSE
  IF NOT EXISTS(SELECT 1 FROM leave_types WHERE id=NEW.leave_type_id AND company_id=NEW.company_id) THEN RAISE EXCEPTION 'LEAVE_COMPANY_MISMATCH'; END IF;
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER leave_assignment_company BEFORE INSERT OR UPDATE ON leave_policy_assignments FOR EACH ROW EXECUTE FUNCTION leave_company_integrity();
CREATE TRIGGER leave_entry_company BEFORE INSERT ON leave_balance_entries FOR EACH ROW EXECUTE FUNCTION leave_company_integrity();
