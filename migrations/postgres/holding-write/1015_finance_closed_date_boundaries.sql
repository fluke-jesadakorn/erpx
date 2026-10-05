-- A retained annual period must not reopen dates in a closed monthly period.
-- Preserve all existing journal/period rows and exports; guard new transitions.
CREATE FUNCTION finance_company_date_boundary() RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
  IF TG_TABLE_NAME='finance_journals' THEN
    IF NEW.status NOT IN ('draft','in_review','posted','reversed') THEN RETURN NEW; END IF;
    IF TG_OP='UPDATE' AND OLD.status IN ('posted','reversed') THEN RETURN NEW; END IF;
    PERFORM pg_advisory_xact_lock(hashtextextended('finance-period-boundary:'||NEW.company_id,0));
    IF EXISTS(SELECT 1 FROM finance_periods p WHERE p.company_id=NEW.company_id AND p.state IN ('closing','closed') AND NEW.entry_date BETWEEN p.start_date AND p.end_date) THEN
      RAISE EXCEPTION 'PERIOD_CLOSED';
    END IF;
  ELSE
    IF NEW.state NOT IN ('closing','closed') OR NEW.state=OLD.state THEN RETURN NEW; END IF;
    PERFORM pg_advisory_xact_lock(hashtextextended('finance-period-boundary:'||NEW.company_id,0));
    IF EXISTS(SELECT 1 FROM finance_journals j WHERE j.company_id=NEW.company_id AND j.status IN ('draft','in_review') AND (j.period_id=NEW.id OR j.entry_date BETWEEN NEW.start_date AND NEW.end_date)) THEN
      RAISE EXCEPTION 'CLOSE_NOT_READY';
    END IF;
  END IF;
  RETURN NEW;
END;
$$;
CREATE TRIGGER finance_post_date_boundary BEFORE INSERT OR UPDATE ON finance_journals
FOR EACH ROW EXECUTE FUNCTION finance_company_date_boundary();
CREATE TRIGGER finance_close_date_boundary BEFORE UPDATE ON finance_periods
FOR EACH ROW EXECUTE FUNCTION finance_company_date_boundary();
