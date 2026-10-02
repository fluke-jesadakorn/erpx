-- Owner workflows used by local hospitality fixtures and durable posting jobs
-- need the same currency evidence access as application commands.
DO $$
DECLARE jobs_role TEXT;
BEGIN
 IF current_user !~ '_migration$' THEN RETURN; END IF;
 jobs_role := regexp_replace(current_user, '_migration$', '_jobs');
 EXECUTE format('GRANT SELECT, INSERT, UPDATE ON company_money_settings,company_currency_scales,user_currency_preferences TO %I',jobs_role);
 EXECUTE format('GRANT SELECT, INSERT ON company_fx_rates,money_transaction_snapshots TO %I',jobs_role);
END $$;
