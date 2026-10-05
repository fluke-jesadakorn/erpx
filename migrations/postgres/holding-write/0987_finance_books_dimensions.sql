-- Additive book projection. Existing journals and their exports are untouched.
CREATE TABLE finance_dimension_definitions (
 id TEXT PRIMARY KEY, company_id TEXT NOT NULL, code TEXT NOT NULL,
 name TEXT NOT NULL, required BIGINT NOT NULL DEFAULT 0 CHECK(required IN (0,1)),
 status TEXT NOT NULL DEFAULT 'active' CHECK(status IN ('active','inactive')),
 version BIGINT NOT NULL DEFAULT 1, created_by TEXT NOT NULL, created_at BIGINT NOT NULL,
 UNIQUE(company_id,code), CHECK(code ~ '^[a-z][a-z0-9_]{0,39}$')
);
CREATE TABLE finance_dimension_values (
 id TEXT PRIMARY KEY, dimension_id TEXT NOT NULL REFERENCES finance_dimension_definitions(id),
 company_id TEXT NOT NULL, code TEXT NOT NULL, name TEXT NOT NULL,
 status TEXT NOT NULL DEFAULT 'active' CHECK(status IN ('active','inactive')),
 version BIGINT NOT NULL DEFAULT 1, created_at BIGINT NOT NULL,
 UNIQUE(dimension_id,code)
);
CREATE TABLE finance_book_rules (
 id TEXT PRIMARY KEY, company_id TEXT NOT NULL,
 primary_chart_id TEXT NOT NULL REFERENCES finance_charts(id),
 secondary_chart_id TEXT NOT NULL REFERENCES finance_charts(id),
 effective_date TEXT NOT NULL, status TEXT NOT NULL DEFAULT 'draft' CHECK(status IN ('draft','approved','rejected')),
 version BIGINT NOT NULL DEFAULT 1, prepared_by TEXT NOT NULL, reviewed_by TEXT,
 reason TEXT NOT NULL, review_reason TEXT, opening_json TEXT NOT NULL CHECK(erpx_json_valid(opening_json)),
 opening_hash TEXT NOT NULL, opening_period_id TEXT REFERENCES finance_periods(id),
 created_at BIGINT NOT NULL, reviewed_at BIGINT,
 CHECK(primary_chart_id<>secondary_chart_id), CHECK(reviewed_by IS NULL OR reviewed_by<>prepared_by),
 UNIQUE(secondary_chart_id,effective_date)
);
CREATE TABLE finance_book_account_mappings (
 rule_id TEXT NOT NULL REFERENCES finance_book_rules(id),
 primary_account_id TEXT NOT NULL REFERENCES finance_accounts(id),
 secondary_account_id TEXT NOT NULL REFERENCES finance_accounts(id),
 PRIMARY KEY(rule_id,primary_account_id), UNIQUE(rule_id,secondary_account_id)
);
CREATE TABLE finance_book_postings (
 primary_journal_id TEXT NOT NULL REFERENCES finance_journals(id),
 secondary_journal_id TEXT NOT NULL UNIQUE REFERENCES finance_journals(id),
 rule_id TEXT NOT NULL REFERENCES finance_book_rules(id),
 company_id TEXT NOT NULL, source_version BIGINT NOT NULL, created_at BIGINT NOT NULL,
 PRIMARY KEY(primary_journal_id,rule_id)
);
CREATE INDEX finance_book_rules_date ON finance_book_rules(company_id,primary_chart_id,effective_date) WHERE status='approved';
CREATE FUNCTION protect_finance_book_rule() RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
 IF TG_OP='DELETE' OR OLD.status<>'draft' OR
    (NEW.company_id,NEW.primary_chart_id,NEW.secondary_chart_id,NEW.effective_date,NEW.prepared_by,NEW.opening_json,NEW.opening_hash,NEW.opening_period_id)
    IS DISTINCT FROM (OLD.company_id,OLD.primary_chart_id,OLD.secondary_chart_id,OLD.effective_date,OLD.prepared_by,OLD.opening_json,OLD.opening_hash,OLD.opening_period_id) THEN
   RAISE EXCEPTION 'BOOK_RULE_IMMUTABLE' USING ERRCODE='23514';
 END IF;
 RETURN NEW;
END; $$;
CREATE TRIGGER finance_book_rule_immutable BEFORE UPDATE OR DELETE ON finance_book_rules FOR EACH ROW EXECUTE FUNCTION protect_finance_book_rule();
CREATE FUNCTION protect_finance_book_mapping() RETURNS TRIGGER LANGUAGE plpgsql AS $$
DECLARE rule finance_book_rules; src finance_accounts; dst finance_accounts;
BEGIN
 IF TG_OP<>'INSERT' THEN RAISE EXCEPTION 'BOOK_MAPPING_IMMUTABLE' USING ERRCODE='23514'; END IF;
 SELECT * INTO rule FROM finance_book_rules WHERE id=NEW.rule_id;
 SELECT * INTO src FROM finance_accounts WHERE id=NEW.primary_account_id;
 SELECT * INTO dst FROM finance_accounts WHERE id=NEW.secondary_account_id;
 IF rule.status<>'draft' OR src.company_id<>rule.company_id OR dst.company_id<>rule.company_id OR
    src.chart_id<>rule.primary_chart_id OR dst.chart_id<>rule.secondary_chart_id OR
    src.category<>dst.category OR src.control_kind<>dst.control_kind OR src.currency IS DISTINCT FROM dst.currency THEN
   RAISE EXCEPTION 'BOOK_MAPPING_SCOPE_OR_ACCOUNT_KIND' USING ERRCODE='23514';
 END IF;
 RETURN NEW;
END; $$;
CREATE TRIGGER finance_book_mapping_immutable BEFORE INSERT OR UPDATE OR DELETE ON finance_book_account_mappings FOR EACH ROW EXECUTE FUNCTION protect_finance_book_mapping();
CREATE TRIGGER finance_book_posting_immutable BEFORE UPDATE OR DELETE ON finance_book_postings FOR EACH ROW EXECUTE FUNCTION protect_financial_event();

-- Deferred execution sees every owner's final lines. Failure rolls back source,
-- payments/allocations, all books, command receipt and financial-event evidence.
CREATE FUNCTION project_finance_books() RETURNS TRIGGER LANGUAGE plpgsql SECURITY DEFINER SET search_path=public AS $$
DECLARE source finance_journals; rule finance_book_rules; child TEXT; line_count BIGINT;
BEGIN
 SELECT * INTO source FROM finance_journals WHERE id=NEW.id;
 IF source.status NOT IN ('posted','reversed') OR source.source_type IN ('book_projection','book_opening','book_adjustment') THEN RETURN NULL; END IF;
 PERFORM pg_advisory_xact_lock(hashtextextended('finance-book-'||source.chart_id,0));
 IF EXISTS(SELECT 1 FROM finance_book_rules WHERE secondary_chart_id=source.chart_id AND status='approved') THEN
   RAISE EXCEPTION 'SECONDARY_BOOK_REQUIRES_OWNED_ADJUSTMENT' USING ERRCODE='23514';
 END IF;
 IF TG_OP='UPDATE' AND OLD.status='posted' AND source.status='reversed' THEN
   UPDATE finance_journals SET status='reversed',version=version+1,reversed_by=source.reversed_by,reversed_at=source.reversed_at,updated_at=source.updated_at
    WHERE id IN (SELECT secondary_journal_id FROM finance_book_postings WHERE primary_journal_id=source.id) AND status='posted';
   RETURN NULL;
 END IF;
 IF TG_OP='UPDATE' AND OLD.status IN ('posted','reversed') THEN RETURN NULL; END IF;
 IF EXISTS(SELECT 1 FROM finance_book_rules WHERE primary_chart_id=source.chart_id AND status='approved' AND effective_date>source.entry_date)
    AND source.entry_date<(SELECT MIN(effective_date) FROM finance_book_rules WHERE primary_chart_id=source.chart_id AND status='approved') THEN
   RAISE EXCEPTION 'BOOK_POSTING_PRECEDES_REVIEWED_OPENING' USING ERRCODE='23514';
 END IF;
 FOR rule IN SELECT DISTINCT ON (secondary_chart_id) * FROM finance_book_rules
   WHERE company_id=source.company_id AND primary_chart_id=source.chart_id AND status='approved' AND effective_date<=source.entry_date
   ORDER BY secondary_chart_id,effective_date DESC LOOP
   PERFORM pg_advisory_xact_lock(hashtextextended('finance-book-'||rule.secondary_chart_id,0));
   IF EXISTS(SELECT 1 FROM finance_book_postings WHERE primary_journal_id=source.id AND rule_id=rule.id) THEN CONTINUE; END IF;
   -- The reviewed opening fixes the carrying amount. Book currency changes need
   -- a distinct carrying-value adapter, never a settlement-date retranslation.
   IF NOT EXISTS(SELECT 1 FROM finance_charts p JOIN finance_charts s ON p.base_currency=s.base_currency
       WHERE p.id=rule.primary_chart_id AND s.id=rule.secondary_chart_id AND p.status='active' AND s.status='active') THEN
     RAISE EXCEPTION 'BOOK_CURRENCY_OR_STATUS_CHANGED' USING ERRCODE='23514';
   END IF;
   SELECT COUNT(*) INTO line_count FROM finance_journal_lines WHERE journal_id=source.id;
   IF line_count<2 OR EXISTS(SELECT 1 FROM finance_journal_lines l LEFT JOIN finance_book_account_mappings m
       ON m.rule_id=rule.id AND m.primary_account_id=l.account_id LEFT JOIN finance_accounts a ON a.id=m.secondary_account_id
       WHERE l.journal_id=source.id AND (m.primary_account_id IS NULL OR a.status<>'active')) THEN
     RAISE EXCEPTION 'BOOK_ACCOUNT_MAPPING_REQUIRED' USING ERRCODE='23514';
   END IF;
   child:='book-'||rule.id||'-'||source.id;
   INSERT INTO finance_journals(id,company_id,chart_id,period_id,journal_number,entry_date,currency,fx_rate_micros,source_type,source_id,memo,status,is_opening,version,created_by,posted_by,posted_at,reversed_by,reversed_at,created_at,updated_at)
    VALUES(child,source.company_id,rule.secondary_chart_id,source.period_id,child,source.entry_date,source.currency,source.fx_rate_micros,'book_projection',source.id,source.memo,source.status,source.is_opening,source.version,source.created_by,source.posted_by,source.posted_at,source.reversed_by,source.reversed_at,source.created_at,source.updated_at);
   INSERT INTO finance_journal_lines(id,journal_id,account_id,description,debit_minor,credit_minor,base_debit_minor,base_credit_minor,dimension_json,ordinal)
    SELECT child||'-'||l.ordinal,child,m.secondary_account_id,l.description,l.debit_minor,l.credit_minor,l.base_debit_minor,l.base_credit_minor,l.dimension_json,l.ordinal
    FROM finance_journal_lines l JOIN finance_book_account_mappings m ON m.rule_id=rule.id AND m.primary_account_id=l.account_id WHERE l.journal_id=source.id;
   INSERT INTO finance_book_postings VALUES(source.id,child,rule.id,source.company_id,source.version,floor(extract(epoch FROM clock_timestamp())*1000)::bigint);
 END LOOP;
 RETURN NULL;
END; $$;
CREATE CONSTRAINT TRIGGER finance_book_projection AFTER INSERT OR UPDATE ON finance_journals
 DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION project_finance_books();

CREATE FUNCTION validate_finance_dimensions() RETURNS TRIGGER LANGUAGE plpgsql AS $$
DECLARE source finance_journals; dims JSONB; item RECORD;
BEGIN
 SELECT * INTO source FROM finance_journals WHERE id=NEW.id;
 IF source.status NOT IN ('posted','reversed') OR (TG_OP='UPDATE' AND OLD.status IN ('posted','reversed')) THEN RETURN NULL; END IF;
 FOR item IN SELECT dimension_json FROM finance_journal_lines WHERE journal_id=source.id LOOP
  dims:=item.dimension_json::jsonb;
  IF jsonb_typeof(dims)<>'object' THEN RAISE EXCEPTION 'DIMENSION_OBJECT_REQUIRED' USING ERRCODE='23514'; END IF;
  IF EXISTS(SELECT 1 FROM finance_dimension_definitions d WHERE d.company_id=source.company_id AND d.status='active' AND d.required=1 AND NOT dims ? d.code) OR
     EXISTS(SELECT 1 FROM jsonb_each_text(dims) kv JOIN finance_dimension_definitions d ON d.code=kv.key AND d.company_id=source.company_id
        WHERE d.status<>'active' OR NOT EXISTS(SELECT 1 FROM finance_dimension_values v WHERE v.dimension_id=d.id AND v.company_id=source.company_id AND v.code=kv.value AND v.status='active')) THEN
    RAISE EXCEPTION 'DIMENSION_VALUE_REQUIRED' USING ERRCODE='23514';
  END IF;
 END LOOP;
 RETURN NULL;
END; $$;
CREATE CONSTRAINT TRIGGER finance_dimension_post_check AFTER INSERT OR UPDATE ON finance_journals
 DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION validate_finance_dimensions();
DO $$
DECLARE app_role TEXT:=regexp_replace(current_user,'_migration$','_app');
BEGIN
 IF right(current_user,10)='_migration' AND EXISTS(SELECT 1 FROM pg_roles WHERE rolname=app_role) THEN
  EXECUTE format('GRANT SELECT,INSERT,UPDATE ON finance_dimension_definitions,finance_dimension_values,finance_book_rules TO %I',app_role);
  EXECUTE format('GRANT SELECT,INSERT ON finance_book_account_mappings TO %I',app_role);
  EXECUTE format('GRANT SELECT ON finance_book_postings TO %I',app_role);
 END IF;
END; $$;
INSERT INTO work_cache_revisions(table_name) VALUES('finance_dimension_definitions'),('finance_dimension_values'),('finance_book_rules'),('finance_book_account_mappings'),('finance_book_postings') ON CONFLICT DO NOTHING;
