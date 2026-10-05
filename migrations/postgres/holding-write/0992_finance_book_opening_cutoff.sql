-- Every enabled book must preserve its separately reviewed opening.
CREATE OR REPLACE FUNCTION project_finance_books() RETURNS TRIGGER LANGUAGE plpgsql SECURITY DEFINER SET search_path=public AS $$
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
    AND source.entry_date<(SELECT MAX(first_date) FROM (SELECT MIN(effective_date) first_date FROM finance_book_rules WHERE primary_chart_id=source.chart_id AND status='approved' GROUP BY secondary_chart_id) starts) THEN
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
