-- Additive statutory evidence; existing gross-pay previews keep their original meaning.
ALTER TABLE hr_country_packs ADD COLUMN adapter_id TEXT;
ALTER TABLE hr_country_packs ADD COLUMN source_digest TEXT CHECK(source_digest IS NULL OR source_digest ~ '^[a-f0-9]{64}$');
ALTER TABLE hr_country_packs ADD COLUMN effective_to TEXT;
ALTER TABLE hr_country_packs ADD COLUMN record_version BIGINT NOT NULL DEFAULT 1 CHECK(record_version>0);

CREATE FUNCTION hr_country_pack_immutable() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN
 IF TG_OP='DELETE' OR (NEW.id,NEW.country_code,NEW.version,NEW.rules_json,NEW.export_formats_json,NEW.created_by,NEW.created_at,NEW.adapter_id,NEW.source_digest,NEW.effective_from,NEW.effective_to)
   IS DISTINCT FROM (OLD.id,OLD.country_code,OLD.version,OLD.rules_json,OLD.export_formats_json,OLD.created_by,OLD.created_at,OLD.adapter_id,OLD.source_digest,OLD.effective_from,OLD.effective_to)
 THEN RAISE EXCEPTION 'COUNTRY_PACK_RULE_REVISION_IMMUTABLE' USING ERRCODE='23514'; END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER hr_country_pack_immutable BEFORE UPDATE OR DELETE ON hr_country_packs FOR EACH ROW EXECUTE FUNCTION hr_country_pack_immutable();

CREATE TABLE hr_thai_payroll_profiles (
 id TEXT PRIMARY KEY,company_id TEXT NOT NULL,engagement_id TEXT NOT NULL REFERENCES hr_engagements(id),
 tax_year BIGINT NOT NULL CHECK(tax_year BETWEEN 2000 AND 2200),effective_from TEXT NOT NULL,effective_to TEXT NOT NULL,
 annual_deductions_minor BIGINT NOT NULL CHECK(annual_deductions_minor>=0),scheduled_pays BIGINT NOT NULL CHECK(scheduled_pays BETWEEN 1 AND 12),
 social_security_eligible BOOLEAN NOT NULL,social_security_exemption_evidence TEXT,
 opening_through_date TEXT NOT NULL,opening_taxable_minor BIGINT NOT NULL CHECK(opening_taxable_minor>=0),
 opening_irregular_minor BIGINT NOT NULL CHECK(opening_irregular_minor>=0 AND opening_irregular_minor<=opening_taxable_minor),
 opening_withheld_minor BIGINT NOT NULL CHECK(opening_withheld_minor>=0),prior_income_complete BOOLEAN NOT NULL,
 evidence_ref TEXT NOT NULL,status TEXT NOT NULL DEFAULT 'draft' CHECK(status IN ('draft','approved','superseded')),
 created_by TEXT NOT NULL,approved_by TEXT,version BIGINT NOT NULL DEFAULT 1,created_at BIGINT NOT NULL,updated_at BIGINT NOT NULL,
 CHECK(effective_from<=effective_to),CHECK(social_security_eligible OR length(trim(social_security_exemption_evidence))>0)
);
CREATE INDEX hr_thai_payroll_profile_lookup ON hr_thai_payroll_profiles(company_id,engagement_id,tax_year,status,effective_from,effective_to);
CREATE FUNCTION hr_thai_payroll_profile_guard() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN
 IF TG_OP='DELETE' OR (NEW.id,NEW.company_id,NEW.engagement_id,NEW.tax_year,NEW.effective_from,NEW.effective_to,NEW.annual_deductions_minor,
 NEW.scheduled_pays,NEW.social_security_eligible,NEW.social_security_exemption_evidence,NEW.opening_through_date,NEW.opening_taxable_minor,
 NEW.opening_irregular_minor,NEW.opening_withheld_minor,NEW.prior_income_complete,NEW.evidence_ref,NEW.created_by,NEW.created_at)
 IS DISTINCT FROM (OLD.id,OLD.company_id,OLD.engagement_id,OLD.tax_year,OLD.effective_from,OLD.effective_to,OLD.annual_deductions_minor,
 OLD.scheduled_pays,OLD.social_security_eligible,OLD.social_security_exemption_evidence,OLD.opening_through_date,OLD.opening_taxable_minor,
 OLD.opening_irregular_minor,OLD.opening_withheld_minor,OLD.prior_income_complete,OLD.evidence_ref,OLD.created_by,OLD.created_at)
 THEN RAISE EXCEPTION 'THAI_PAYROLL_PROFILE_REVISION_IMMUTABLE' USING ERRCODE='23514'; END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER hr_thai_payroll_profile_guard BEFORE UPDATE OR DELETE ON hr_thai_payroll_profiles FOR EACH ROW EXECUTE FUNCTION hr_thai_payroll_profile_guard();
DO $$ DECLARE t TEXT; BEGIN FOREACH t IN ARRAY ARRAY['hr_thai_payroll_profiles'] LOOP
 INSERT INTO work_cache_revisions(table_name) VALUES(t) ON CONFLICT DO NOTHING;
 EXECUTE format('CREATE TRIGGER work_cache_change AFTER INSERT OR UPDATE OR DELETE ON %I FOR EACH ROW EXECUTE FUNCTION work_cache_changed()',t);
 EXECUTE format('CREATE TRIGGER work_cache_truncate AFTER TRUNCATE ON %I FOR EACH STATEMENT EXECUTE FUNCTION work_cache_changed()',t);
END LOOP;
END $$;
DO $$ DECLARE runtime_role TEXT; BEGIN
 IF current_user !~ '_migration$' THEN RETURN; END IF;
 FOREACH runtime_role IN ARRAY ARRAY[regexp_replace(current_user,'_migration$','_app'),regexp_replace(current_user,'_migration$','_jobs')] LOOP
  EXECUTE format('GRANT SELECT,INSERT,UPDATE ON hr_thai_payroll_profiles TO %I',runtime_role);
 END LOOP;
END $$;
