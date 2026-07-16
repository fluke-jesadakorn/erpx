CREATE OR REPLACE FUNCTION folio.waybill_risk_score(p_waybill_id text)
RETURNS int
LANGUAGE plpgsql
STABLE
AS $$
DECLARE
  v_submitter int;
  v_vendor text;
  v_amount numeric;
  v_transaction_date date;
  v_current_stage text;
  v_ocr_confidence numeric;
  v_rejection_count int;
  v_vendor_freq int;
  v_avg numeric;
  v_hour int;
  v_score int := 0;
BEGIN
  SELECT w.submitter_id, COALESCE(w.vendor_name, e.vendor_name), w.total_amount::numeric, e.transaction_date, w.current_stage
    INTO v_submitter, v_vendor, v_amount, v_transaction_date, v_current_stage
    FROM folio.waybills w
    LEFT JOIN folio.expenses e ON e.id = w.origin_id AND w.origin = 'expense'
   WHERE w.id = p_waybill_id;

  IF v_submitter IS NULL THEN RETURN 0; END IF;

  IF v_vendor IS NOT NULL THEN
    SELECT COUNT(*) INTO v_vendor_freq
      FROM folio.expenses
     WHERE submitter_id = v_submitter
       AND vendor_name = v_vendor
       AND created_at > now() - INTERVAL '90 days';
    IF v_vendor_freq >= 10 THEN v_score := v_score + 25;
    ELSIF v_vendor_freq >= 5 THEN v_score := v_score + 15;
    ELSIF v_vendor_freq >= 3 THEN v_score := v_score + 5;
    END IF;
  END IF;

  IF v_amount IS NOT NULL AND v_amount > 0 THEN
    SELECT COALESCE(AVG(total_amount), 0) INTO v_avg
      FROM folio.expenses
     WHERE submitter_id = v_submitter
       AND created_at > now() - INTERVAL '180 days';
    IF v_avg > 0 THEN
      IF v_amount > v_avg * 10 THEN v_score := v_score + 25;
      ELSIF v_amount > v_avg * 5  THEN v_score := v_score + 15;
      ELSIF v_amount > v_avg * 2  THEN v_score := v_score + 5;
      END IF;
    END IF;
  END IF;

  IF v_transaction_date IS NOT NULL THEN
    v_hour := EXTRACT(HOUR FROM v_transaction_date AT TIME ZONE 'UTC')::int;
    IF v_hour >= 22 OR v_hour < 4 THEN v_score := v_score + 5; END IF;
  END IF;

  IF v_current_stage = 'expense' OR v_current_stage IN ('submission','dept_verification') THEN
    SELECT COALESCE(s.ocr_confidence, 0) INTO v_ocr_confidence
      FROM folio.slips s
      JOIN folio.expenses e ON e.id = s.expense_id
      JOIN folio.waybills w ON w.origin_id = e.id AND w.origin = 'expense'
     WHERE w.id = p_waybill_id
     ORDER BY s.id DESC
     LIMIT 1;
    IF v_ocr_confidence IS NOT NULL AND v_ocr_confidence < 0.6 THEN v_score := v_score + 20;
    ELSIF v_ocr_confidence IS NOT NULL AND v_ocr_confidence < 0.8 THEN v_score := v_score + 10;
    END IF;
  END IF;

  SELECT COUNT(*) INTO v_rejection_count
    FROM folio.waybill_events
   WHERE waybill_id = p_waybill_id AND kind IN ('rejected','so-rejected');

  IF v_rejection_count >= 1 THEN v_score := v_score + 15; END IF;

  IF v_score > 100 THEN v_score := 100; END IF;
  RETURN v_score;
END;
$$;

CREATE OR REPLACE VIEW folio.waybill_risk AS
  SELECT id, folio.waybill_risk_score(id) AS risk_score
    FROM folio.waybills;

CREATE TABLE IF NOT EXISTS folio.approver_nudges (
  id bigserial PRIMARY KEY,
  approver_user_id int NOT NULL REFERENCES folio.users(id) ON DELETE CASCADE,
  waybill_id text NOT NULL,
  stage text NOT NULL,
  hint text NOT NULL,
  sent_at timestamptz NOT NULL DEFAULT now(),
  UNIQUE (approver_user_id, waybill_id, stage)
);

CREATE INDEX IF NOT EXISTS folio_approver_nudges_approver_idx
  ON folio.approver_nudges(approver_user_id, sent_at DESC);