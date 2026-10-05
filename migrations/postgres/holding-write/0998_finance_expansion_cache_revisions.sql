-- New review queues must invalidate the same shared work cache as their source owners.
DO $$ DECLARE t TEXT;BEGIN
 FOREACH t IN ARRAY ARRAY['finance_dimension_definitions','finance_dimension_values','finance_book_rules','finance_book_account_mappings','finance_book_postings','finance_plan_editions','finance_report_snapshots','finance_bank_csv_mappings','finance_bank_match_groups','finance_bank_match_targets','finance_collection_cases','finance_company_tools','finance_business_contracts','finance_business_billing_runs','finance_recognition_runs','finance_book_adjustments','finance_instrument_editions','finance_instrument_runs','finance_asset_policy_editions','finance_asset_policy_runs'] LOOP
  IF to_regclass(t) IS NULL THEN CONTINUE;END IF;
  INSERT INTO work_cache_revisions(table_name) VALUES(t) ON CONFLICT DO NOTHING;
  IF NOT EXISTS(SELECT 1 FROM pg_trigger WHERE tgrelid=to_regclass(t) AND tgname='work_cache_change') THEN EXECUTE format('CREATE TRIGGER work_cache_change AFTER INSERT OR UPDATE OR DELETE ON %I FOR EACH ROW EXECUTE FUNCTION work_cache_changed()',t);END IF;
  IF NOT EXISTS(SELECT 1 FROM pg_trigger WHERE tgrelid=to_regclass(t) AND tgname='work_cache_truncate') THEN EXECUTE format('CREATE TRIGGER work_cache_truncate AFTER TRUNCATE ON %I FOR EACH STATEMENT EXECUTE FUNCTION work_cache_changed()',t);END IF;
 END LOOP;
END;$$;
