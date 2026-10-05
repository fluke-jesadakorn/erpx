-- Account owners may update both ends in one transaction, but an enabled
-- projection cannot silently change a control account's meaning on one side.
CREATE FUNCTION verify_finance_book_account_identity() RETURNS TRIGGER LANGUAGE plpgsql SECURITY DEFINER SET search_path=public AS $$
BEGIN
 IF EXISTS(SELECT 1 FROM finance_book_account_mappings m JOIN finance_book_rules r ON r.id=m.rule_id
   JOIN finance_accounts p ON p.id=m.primary_account_id JOIN finance_accounts s ON s.id=m.secondary_account_id
   WHERE r.status='approved' AND (p.id=NEW.id OR s.id=NEW.id) AND
    (p.company_id<>r.company_id OR s.company_id<>r.company_id OR p.chart_id<>r.primary_chart_id OR s.chart_id<>r.secondary_chart_id
     OR p.category<>s.category OR p.control_kind<>s.control_kind OR p.currency IS DISTINCT FROM s.currency)) THEN
   RAISE EXCEPTION 'BOOK_ACCOUNT_IDENTITY_CHANGED' USING ERRCODE='23514';
 END IF;
 RETURN NULL;
END; $$;
CREATE CONSTRAINT TRIGGER finance_book_account_identity AFTER UPDATE ON finance_accounts
 DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION verify_finance_book_account_identity();
CREATE FUNCTION verify_finance_book_activation() RETURNS TRIGGER LANGUAGE plpgsql SECURITY DEFINER SET search_path=public AS $$
BEGIN
 IF NEW.status<>'approved' THEN RETURN NULL; END IF;
 IF NOT EXISTS(SELECT 1 FROM finance_charts p JOIN finance_charts s ON s.base_currency=p.base_currency
   WHERE p.id=NEW.primary_chart_id AND s.id=NEW.secondary_chart_id AND p.company_id=NEW.company_id AND s.company_id=NEW.company_id AND p.status='active' AND s.status='active') OR
  EXISTS(SELECT 1 FROM finance_accounts p WHERE p.chart_id=NEW.primary_chart_id AND p.status='active' AND NOT EXISTS(
   SELECT 1 FROM finance_book_account_mappings m JOIN finance_accounts s ON s.id=m.secondary_account_id WHERE m.rule_id=NEW.id AND m.primary_account_id=p.id
    AND s.status='active' AND s.company_id=NEW.company_id AND s.chart_id=NEW.secondary_chart_id AND s.category=p.category AND s.control_kind=p.control_kind AND s.currency IS NOT DISTINCT FROM p.currency)) THEN
  RAISE EXCEPTION 'BOOK_ACCOUNT_MAPPING_REQUIRED' USING ERRCODE='23514';
 END IF;
 RETURN NULL;
END; $$;
CREATE CONSTRAINT TRIGGER finance_book_activation_check AFTER INSERT OR UPDATE ON finance_book_rules
 DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION verify_finance_book_activation();
