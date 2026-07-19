\set ON_ERROR_STOP on

BEGIN;

CREATE EXTENSION IF NOT EXISTS "uuid-ossp" WITH SCHEMA public;
CREATE EXTENSION IF NOT EXISTS vector WITH SCHEMA public;
CREATE EXTENSION IF NOT EXISTS pgcrypto WITH SCHEMA public;

-- Generated from the current Folio schema on PostgreSQL 18.


-- Dumped from database version 18.4 (Homebrew)
-- Dumped by pg_dump version 18.4 (Homebrew)

SET statement_timeout = 0;
SET lock_timeout = 0;
SET idle_in_transaction_session_timeout = 0;
SET transaction_timeout = 0;
SET client_encoding = 'UTF8';
SET standard_conforming_strings = on;
SELECT pg_catalog.set_config('search_path', '', false);
SET check_function_bodies = false;
SET xmloption = content;
SET client_min_messages = warning;
SET row_security = off;

--
-- Name: ai; Type: SCHEMA; Schema: -; Owner: -
--

CREATE SCHEMA ai;


--
-- Name: auth; Type: SCHEMA; Schema: -; Owner: -
--

CREATE SCHEMA auth;


--
-- Name: chat; Type: SCHEMA; Schema: -; Owner: -
--

CREATE SCHEMA chat;


--
-- Name: finance; Type: SCHEMA; Schema: -; Owner: -
--

CREATE SCHEMA finance;


--
-- Name: folio; Type: SCHEMA; Schema: -; Owner: -
--

CREATE SCHEMA folio;


--
-- Name: hook; Type: SCHEMA; Schema: -; Owner: -
--

CREATE SCHEMA hook;


--
-- Name: law; Type: SCHEMA; Schema: -; Owner: -
--

CREATE SCHEMA law;


--
-- Name: n8n; Type: SCHEMA; Schema: -; Owner: -
--

CREATE SCHEMA n8n;


--
-- Name: perm; Type: SCHEMA; Schema: -; Owner: -
--

CREATE SCHEMA perm;


--
-- Name: recompute_model_ratings(); Type: FUNCTION; Schema: ai; Owner: -
--

CREATE FUNCTION ai.recompute_model_ratings() RETURNS void
    LANGUAGE sql
    AS $$
  INSERT INTO ai.model_ratings (model_name, speed, accuracy, computed_at)
  SELECT
    m.name,
    LEAST(5, GREATEST(1,
      CASE WHEN COALESCE(AVG(i.latency_ms), 0) = 0 THEN 3
           ELSE GREATEST(1, LEAST(5, ROUND(2000.0 / AVG(i.latency_ms))::int))
      END
    )) AS speed,
    LEAST(5.00, GREATEST(0.00,
      ROUND(5.0 * SUM(CASE WHEN i.status='ok' THEN 1 ELSE 0 END)::numeric
            / NULLIF(COUNT(*), 0), 2)
    )) AS accuracy,
    now()
  FROM ai_models m
  JOIN ai_invocations i ON i.model_id = m.id
  WHERE i.created_at >= now() - INTERVAL '30 days'
  GROUP BY m.name
  ON CONFLICT (model_name) DO UPDATE
    SET speed = EXCLUDED.speed,
        accuracy = EXCLUDED.accuracy,
        computed_at = now();
$$;


--
-- Name: ai_decrypt(bytea, text); Type: FUNCTION; Schema: folio; Owner: -
--

CREATE FUNCTION folio.ai_decrypt(cipher bytea, key text) RETURNS text
    LANGUAGE sql
    AS $$
  SELECT pgp_sym_decrypt(cipher, key)
$$;


--
-- Name: ai_encrypt(text, text); Type: FUNCTION; Schema: folio; Owner: -
--

CREATE FUNCTION folio.ai_encrypt(plain text, key text) RETURNS bytea
    LANGUAGE sql
    AS $$
  SELECT pgp_sym_encrypt(plain, key)
$$;


--
-- Name: backfill_exec_snapshot(date, numeric, numeric); Type: FUNCTION; Schema: folio; Owner: -
--

CREATE FUNCTION folio.backfill_exec_snapshot(target_date date, cash_val numeric, mtd_val numeric) RETURNS void
    LANGUAGE sql
    AS $$
  INSERT INTO exec_snapshots (
    snapshot_date,
    kpis,
    dept_budgets,
    stuck_count
  ) VALUES (
    target_date,
    jsonb_build_object(
      'cash', jsonb_build_object(
        'totalCash',               cash_val,
        'outstandingLiabilities',  0,
        'mtdExpenses',             mtd_val,
        'netIncome',               0
      ),
      'kpis',       jsonb_build_object('mtdExpenses', mtd_val),
      'cashTrend',  jsonb_build_array(cash_val),
      'mtdTrend',   jsonb_build_array(mtd_val)
    ),
    '[]'::jsonb,
    0
  )
  ON CONFLICT (snapshot_date) DO NOTHING;
$$;


--
-- Name: get_ar_aging_buckets(); Type: FUNCTION; Schema: folio; Owner: -
--

CREATE FUNCTION folio.get_ar_aging_buckets() RETURNS TABLE(bucket text, days_from integer, days_to integer, amount_thb numeric, so_count bigint)
    LANGUAGE sql STABLE
    AS $$
  SELECT
    bucket,
    days_from,
    days_to,
    amount_thb,
    so_count
  FROM (
    SELECT
      CASE
        WHEN days_overdue BETWEEN 0 AND 30  THEN '0-30'
        WHEN days_overdue BETWEEN 31 AND 60 THEN '31-60'
        WHEN days_overdue BETWEEN 61 AND 90 THEN '61-90'
        ELSE '90+'
      END                                                AS bucket,
      CASE
        WHEN days_overdue BETWEEN 0  AND 30 THEN 0
        WHEN days_overdue BETWEEN 31 AND 60 THEN 31
        WHEN days_overdue BETWEEN 61 AND 90 THEN 61
        ELSE 90
      END                                                AS days_from,
      CASE
        WHEN days_overdue BETWEEN 0  AND 30 THEN 30
        WHEN days_overdue BETWEEN 31 AND 60 THEN 60
        WHEN days_overdue BETWEEN 61 AND 90 THEN 90
        ELSE 9999
      END                                                AS days_to,
      SUM(total_amount)::numeric(14,2)                   AS amount_thb,
      COUNT(*)::bigint                                   AS so_count
    FROM (
      SELECT
        so.id,
        so.total_amount,
        GREATEST(0, (CURRENT_DATE - so.due_date)::int) AS days_overdue
      FROM sales_orders so
      WHERE so.status IN ('so_invoiced', 'so_credit_check')
        AND so.due_date IS NOT NULL
        AND so.due_date <= CURRENT_DATE
    ) aged
    GROUP BY 1, 2, 3
  ) b
  ORDER BY days_from;
$$;


--
-- Name: get_cockpit_projection(integer); Type: FUNCTION; Schema: folio; Owner: -
--

CREATE FUNCTION folio.get_cockpit_projection(days_ahead integer DEFAULT 90) RETURNS jsonb
    LANGUAGE sql STABLE
    AS $$
WITH series AS (
  SELECT
    snapshot_date                                         AS d,
    (kpis->'cash'->>'totalCash')::numeric                 AS cash,
    (kpis->'kpis'->>'mtdExpenses')::numeric               AS mtd,
    EXTRACT(EPOCH FROM snapshot_date)::numeric / 86400    AS x
  FROM exec_snapshots
),
regs AS (
  SELECT
    COALESCE(regr_slope    (cash, x), 0)::numeric AS cash_slope,
    COALESCE(regr_intercept(cash, x), 0)::numeric AS cash_intercept,
    COALESCE(regr_r2       (cash, x), 0)::numeric AS cash_r2,
    COALESCE(regr_slope    (mtd,  x), 0)::numeric AS mtd_slope,
    COALESCE(regr_intercept(mtd,  x), 0)::numeric AS mtd_intercept,
    COALESCE(regr_r2       (mtd,  x), 0)::numeric AS mtd_r2
  FROM series
),
proj AS (
  SELECT
    gs::int                                                       AS offset_days,
    (CURRENT_DATE + gs)::date                                     AS pd,
    EXTRACT(EPOCH FROM (CURRENT_DATE + gs)::date)::numeric / 86400 AS px,
    r.cash_slope, r.cash_intercept, r.mtd_slope, r.mtd_intercept
  FROM generate_series(1, days_ahead) gs
  CROSS JOIN regs r
)
SELECT jsonb_build_object(
  'historical', COALESCE(
    (SELECT jsonb_agg(
       jsonb_build_object('date', d::text, 'cash', cash, 'mtd', mtd)
       ORDER BY d)
     FROM series),
    '[]'::jsonb),
  'regression', jsonb_build_object(
    'cash', jsonb_build_object(
      'slope',     cash_slope,
      'intercept', cash_intercept,
      'r2',        cash_r2),
    'mtd',  jsonb_build_object(
      'slope',     mtd_slope,
      'intercept', mtd_intercept,
      'r2',        mtd_r2)
  ),
  'projection', COALESCE(
    (SELECT jsonb_agg(
       jsonb_build_object(
         'date',           pd::text,
         'cashProjected',  ROUND(cash_slope * px + cash_intercept, 2),
         'mtdProjected',   ROUND(mtd_slope  * px + mtd_intercept,  2),
         'isProjected',    true)
       ORDER BY offset_days)
     FROM proj),
    '[]'::jsonb),
  'summary', jsonb_build_object(
    'currentCash',     COALESCE((SELECT cash FROM series ORDER BY d DESC LIMIT 1), 0),
    'currentMtd',      COALESCE((SELECT mtd  FROM series ORDER BY d DESC LIMIT 1), 0),
    'projectedCash30', ROUND(cash_slope * (EXTRACT(EPOCH FROM (CURRENT_DATE + 30)::date)::numeric / 86400) + cash_intercept, 2),
    'projectedCash60', ROUND(cash_slope * (EXTRACT(EPOCH FROM (CURRENT_DATE + 60)::date)::numeric / 86400) + cash_intercept, 2),
    'projectedCash90', ROUND(cash_slope * (EXTRACT(EPOCH FROM (CURRENT_DATE + 90)::date)::numeric / 86400) + cash_intercept, 2),
    'monthlyBurn',     ROUND(cash_slope * 30, 2),
    'daysToZero',      CASE
                         WHEN cash_slope < 0
                           THEN ROUND(
                             ABS(
                               (0 - COALESCE((SELECT cash FROM series ORDER BY d DESC LIMIT 1), 0))::numeric
                               / cash_slope
                             )
                           )::int
                         ELSE NULL
                       END,
    'trend', CASE
               WHEN cash_slope < -1 THEN 'down'
               WHEN cash_slope >  1 THEN 'up'
               ELSE 'flat'
             END,
    'r2', cash_r2
  )
) FROM regs;
$$;


--
-- Name: get_dept_budget_status(integer, integer); Type: FUNCTION; Schema: folio; Owner: -
--

CREATE FUNCTION folio.get_dept_budget_status(p_fiscal_year integer, p_month integer) RETURNS TABLE(dept_id text, dept_name text, monthly_budget numeric, mtd_spend numeric, pct_used numeric, is_over_threshold boolean)
    LANGUAGE sql STABLE
    AS $_$
  WITH dept_users AS (
    SELECT DISTINCT regexp_replace(permission_id, '^user:dept:([^:]+)::allow$', '\1') AS dept_id,
           up.user_id
      FROM perm.user_permissions up
     WHERE permission_id LIKE 'user:dept:%::allow'
       AND revoked_at IS NULL
  ),
  depts AS (
    SELECT d.dept_id,
           COALESCE(MAX(r.monthly_budget), 0)::numeric AS monthly_budget,
           COALESCE(MAX(r.display_name), initcap(replace(d.dept_id, '-', ' '))) AS display_name
      FROM (SELECT DISTINCT dept_id FROM dept_users) d
      LEFT JOIN perm.roles r ON r.id = 'dept-' || d.dept_id
     GROUP BY d.dept_id
  )
  SELECT d.dept_id,
         d.display_name,
         d.monthly_budget,
         COALESCE(SUM(e.total_amount) FILTER (
           WHERE EXTRACT(YEAR  FROM e.created_at) = p_fiscal_year
             AND EXTRACT(MONTH FROM e.created_at) = p_month
             AND e.status NOT IN ('rejected','draft')
         ), 0)::numeric AS mtd_spend,
         CASE
           WHEN d.monthly_budget > 0
             THEN ROUND((COALESCE(SUM(e.total_amount) FILTER (
               WHERE EXTRACT(YEAR  FROM e.created_at) = p_fiscal_year
                 AND EXTRACT(MONTH FROM e.created_at) = p_month
                 AND e.status NOT IN ('rejected','draft')
             ), 0) / d.monthly_budget * 100)::numeric, 1)
           ELSE 0
         END AS pct_used,
         CASE
           WHEN d.monthly_budget > 0
             THEN COALESCE(SUM(e.total_amount) FILTER (
               WHERE EXTRACT(YEAR  FROM e.created_at) = p_fiscal_year
                 AND EXTRACT(MONTH FROM e.created_at) = p_month
                 AND e.status NOT IN ('rejected','draft')
             ), 0) > (d.monthly_budget * 0.9)
           ELSE false
         END AS is_over_threshold
    FROM depts d
    LEFT JOIN dept_users du ON du.dept_id = d.dept_id
    LEFT JOIN expenses e ON e.submitter_id = du.user_id
   GROUP BY d.dept_id, d.display_name, d.monthly_budget
   ORDER BY d.display_name;
$_$;


--
-- Name: next_purchase_order_number(integer); Type: FUNCTION; Schema: folio; Owner: -
--

CREATE FUNCTION folio.next_purchase_order_number(p_year integer) RETURNS text
    LANGUAGE plpgsql
    AS $$
DECLARE n INT;
BEGIN
  SELECT COALESCE(MAX(
    CAST(regexp_replace(po_number, '^PO-'||p_year||'-', '') AS INT)
  ), 0) + 1 INTO n
  FROM purchase_orders
  WHERE po_number LIKE 'PO-'||p_year||'-%';
  RETURN 'PO-'||p_year||'-'||lpad(n::text, 6, '0');
END $$;


--
-- Name: next_sales_order_number(smallint); Type: FUNCTION; Schema: folio; Owner: -
--

CREATE FUNCTION folio.next_sales_order_number(p_fiscal_year smallint) RETURNS text
    LANGUAGE plpgsql
    AS $$
DECLARE
  seq_name text := 'sales_orders_fy_' || p_fiscal_year || '_seq';
  next_n   int;
  result   text;
BEGIN
  IF NOT EXISTS (SELECT 1 FROM pg_class WHERE relname = seq_name) THEN
    EXECUTE 'CREATE SEQUENCE ' || seq_name || ' START 1 INCREMENT 1 NO CYCLE';
  END IF;
  EXECUTE 'SELECT nextval(' || quote_literal(seq_name) || ')' INTO next_n;
  result := 'SO-' || p_fiscal_year::text || '-' || lpad(next_n::text, 6, '0');
  RETURN result;
END
$$;


--
-- Name: next_waybill_number(smallint); Type: FUNCTION; Schema: folio; Owner: -
--

CREATE FUNCTION folio.next_waybill_number(p_fiscal_year smallint) RETURNS text
    LANGUAGE plpgsql
    AS $$
DECLARE
  seq_name text := 'waybills_fy_' || p_fiscal_year || '_seq';
  next_n   int;
  result   text;
BEGIN
  IF NOT EXISTS (SELECT 1 FROM pg_class WHERE relname = seq_name) THEN
    EXECUTE 'CREATE SEQUENCE ' || seq_name || ' START 1 INCREMENT 1 NO CYCLE';
  END IF;
  EXECUTE 'SELECT nextval(' || quote_literal(seq_name) || ')' INTO next_n;
  result := 'WB-' || p_fiscal_year::text || '-' || lpad(next_n::text, 6, '0');
  RETURN result;
END
$$;


--
-- Name: slips_check_exactly_one_parent(); Type: FUNCTION; Schema: folio; Owner: -
--

CREATE FUNCTION folio.slips_check_exactly_one_parent() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
DECLARE
  current_row   slips%ROWTYPE;
  parent_count  INT;
  bad_cols      TEXT;
BEGIN
  SELECT * INTO current_row FROM slips WHERE id = NEW.id;

  -- pending slips may have zero parents — parent is assigned at confirm-time
  IF current_row.status = 'pending' THEN
    RETURN NEW;
  END IF;

  parent_count := (CASE WHEN current_row.expense_id IS NOT NULL THEN 1 ELSE 0 END) +
                  (CASE WHEN current_row.pr_id      IS NOT NULL THEN 1 ELSE 0 END) +
                  (CASE WHEN current_row.po_id      IS NOT NULL THEN 1 ELSE 0 END);
  IF parent_count <> 1 THEN
    bad_cols := concat_ws(',',
      CASE WHEN current_row.expense_id IS NOT NULL THEN 'expense_id' END,
      CASE WHEN current_row.pr_id      IS NOT NULL THEN 'pr_id'      END,
      CASE WHEN current_row.po_id      IS NOT NULL THEN 'po_id'      END);
    RAISE EXCEPTION 'slips_exactly_one_parent: expected exactly one parent, got % (set: %)',
      parent_count, COALESCE(bad_cols, '(none)')
      USING ERRCODE = 'check_violation';
  END IF;
  RETURN NEW;
END;
$$;


--
-- Name: waybill_risk_score(text); Type: FUNCTION; Schema: folio; Owner: -
--

CREATE FUNCTION folio.waybill_risk_score(p_waybill_id text) RETURNS integer
    LANGUAGE plpgsql STABLE
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


--
-- Name: next_doc_seq(); Type: FUNCTION; Schema: law; Owner: -
--

CREATE FUNCTION law.next_doc_seq() RETURNS text
    LANGUAGE plpgsql
    AS $$
DECLARE
  today text := to_char(now() AT TIME ZONE 'Asia/Bangkok', 'YYYYMMDD');
  seq integer;
BEGIN
  PERFORM pg_advisory_xact_lock(hashtext('law.doc_no_seq'));
  SELECT COALESCE(MAX(CAST(substring(doc_no FROM 'DOC-[0-9]{8}-([0-9]+)') AS integer)), 0) + 1
    INTO seq
    FROM law.contracts
   WHERE doc_no LIKE 'DOC-' || today || '-%';
  RETURN 'DOC-' || today || '-' || lpad(seq::text, 4, '0');
END;
$$;


--
-- Name: touch_updated_at(); Type: FUNCTION; Schema: law; Owner: -
--

CREATE FUNCTION law.touch_updated_at() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
BEGIN
  NEW.updated_at := now();
  RETURN NEW;
END;
$$;


--
-- Name: touch_updated_at(); Type: FUNCTION; Schema: perm; Owner: -
--

CREATE FUNCTION perm.touch_updated_at() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
BEGIN NEW.updated_at = now(); RETURN NEW; END;
$$;


SET default_tablespace = '';

SET default_table_access_method = heap;

--
-- Name: model_ratings; Type: TABLE; Schema: ai; Owner: -
--

CREATE TABLE ai.model_ratings (
    model_name text NOT NULL,
    speed integer NOT NULL,
    accuracy numeric(4,2) NOT NULL,
    computed_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT model_ratings_accuracy_check CHECK (((accuracy >= (0)::numeric) AND (accuracy <= (5)::numeric))),
    CONSTRAINT model_ratings_speed_check CHECK (((speed >= 1) AND (speed <= 5)))
);


--
-- Name: sessions; Type: TABLE; Schema: auth; Owner: -
--

CREATE TABLE auth.sessions (
    id text NOT NULL,
    user_id integer NOT NULL,
    impersonator_user_id integer,
    role text NOT NULL,
    issued_at timestamp with time zone DEFAULT now() NOT NULL,
    expires_at timestamp with time zone NOT NULL,
    revoked_at timestamp with time zone,
    last_seen_at timestamp with time zone DEFAULT now() NOT NULL,
    last_seen_ip inet,
    last_seen_ua text,
    locale text DEFAULT 'th'::text NOT NULL,
    CONSTRAINT sessions_locale_check CHECK ((locale = ANY (ARRAY['th'::text, 'de'::text])))
);


--
-- Name: messages; Type: TABLE; Schema: chat; Owner: -
--

CREATE TABLE chat.messages (
    id bigint NOT NULL,
    session_id uuid NOT NULL,
    role text NOT NULL,
    content text NOT NULL,
    blocks jsonb DEFAULT '{}'::jsonb NOT NULL,
    model_name text,
    latency_ms integer,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    reasoning text,
    CONSTRAINT messages_role_check CHECK ((role = ANY (ARRAY['user'::text, 'assistant'::text, 'system'::text])))
);


--
-- Name: messages_id_seq; Type: SEQUENCE; Schema: chat; Owner: -
--

CREATE SEQUENCE chat.messages_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: messages_id_seq; Type: SEQUENCE OWNED BY; Schema: chat; Owner: -
--

ALTER SEQUENCE chat.messages_id_seq OWNED BY chat.messages.id;


--
-- Name: sessions; Type: TABLE; Schema: chat; Owner: -
--

CREATE TABLE chat.sessions (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    user_id integer NOT NULL,
    title text DEFAULT 'New chat'::text NOT NULL,
    model_name text DEFAULT 'openrouter/free'::text NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    scope text DEFAULT 'global'::text NOT NULL
);


--
-- Name: account_cf_class; Type: TABLE; Schema: finance; Owner: -
--

CREATE TABLE finance.account_cf_class (
    account_code character varying(20) NOT NULL,
    activity character varying(20) NOT NULL,
    is_cash_account boolean DEFAULT false NOT NULL,
    note text,
    effective_from date DEFAULT '0001-01-01'::date NOT NULL,
    effective_to date DEFAULT '9999-12-31'::date NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_by integer,
    CONSTRAINT account_cf_class_activity_check CHECK (((activity)::text = ANY ((ARRAY['operating'::character varying, 'investing'::character varying, 'financing'::character varying, 'non_cash'::character varying, 'unclassified'::character varying])::text[])))
);


--
-- Name: ai_provider_health; Type: TABLE; Schema: finance; Owner: -
--

CREATE TABLE finance.ai_provider_health (
    provider_id integer NOT NULL,
    ok boolean NOT NULL,
    model_count integer,
    latency_ms integer,
    error text,
    checked_at timestamp without time zone DEFAULT now() NOT NULL
);


--
-- Name: ai_user_section_defaults; Type: TABLE; Schema: finance; Owner: -
--

CREATE TABLE finance.ai_user_section_defaults (
    user_id integer NOT NULL,
    section_key text NOT NULL,
    model_id integer NOT NULL,
    updated_at timestamp without time zone DEFAULT now() NOT NULL
);


--
-- Name: cashflow_period; Type: TABLE; Schema: finance; Owner: -
--

CREATE TABLE finance.cashflow_period (
    fiscal_year integer NOT NULL,
    period_start date NOT NULL,
    period_end date NOT NULL,
    status character varying(16) DEFAULT 'open'::character varying NOT NULL,
    opening_balance_journal_id integer,
    opened_by integer,
    opened_at timestamp with time zone DEFAULT now() NOT NULL,
    closed_at timestamp with time zone,
    closed_by integer,
    notes text,
    CONSTRAINT cashflow_period_status_check CHECK (((status)::text = ANY ((ARRAY['open'::character varying, 'closed'::character varying, 'locked'::character varying])::text[])))
);


--
-- Name: chart_of_accounts; Type: TABLE; Schema: folio; Owner: -
--

CREATE TABLE folio.chart_of_accounts (
    code character varying(20) NOT NULL,
    name character varying(100) NOT NULL,
    name_th character varying(100) NOT NULL,
    account_type character varying(50) NOT NULL,
    embedding public.vector(1024),
    normal_side text DEFAULT 'debit'::text NOT NULL,
    CONSTRAINT chart_of_accounts_normal_side_chk CHECK ((normal_side = ANY (ARRAY['debit'::text, 'credit'::text])))
);


--
-- Name: journal_entries; Type: TABLE; Schema: folio; Owner: -
--

CREATE TABLE folio.journal_entries (
    id integer NOT NULL,
    expense_id integer,
    entry_date date DEFAULT CURRENT_DATE NOT NULL,
    description character varying(255) NOT NULL,
    created_at timestamp without time zone DEFAULT CURRENT_TIMESTAMP,
    is_draft boolean DEFAULT false NOT NULL,
    finalized_at timestamp without time zone,
    finalized_by integer,
    draft_source text DEFAULT 'expense'::text NOT NULL,
    pr_id integer,
    po_id integer,
    step text DEFAULT 'reimbursement'::text NOT NULL,
    so_id integer,
    prepared_by integer,
    approved_by integer,
    ai_suggestion jsonb,
    ai_confidence numeric(5,4),
    CONSTRAINT journal_entries_draft_source_chk CHECK (((draft_source IS NULL) OR (draft_source = ANY (ARRAY['expense'::text, 'pr'::text, 'po'::text, 'so'::text])))),
    CONSTRAINT journal_entries_step_check CHECK ((step = ANY (ARRAY['reimbursement'::text, 'accrual'::text, 'settlement'::text, 'sales_vat'::text, 'sales_accrual'::text, 'sales_settlement'::text])))
);


--
-- Name: ledger_lines; Type: TABLE; Schema: folio; Owner: -
--

CREATE TABLE folio.ledger_lines (
    id integer NOT NULL,
    journal_entry_id integer,
    account_code character varying(20),
    debit numeric(12,2) DEFAULT 0.00 NOT NULL,
    credit numeric(12,2) DEFAULT 0.00 NOT NULL,
    description character varying(255),
    CONSTRAINT chk_debit_credit CHECK ((((debit >= (0)::numeric) AND (credit = (0)::numeric)) OR ((credit >= (0)::numeric) AND (debit = (0)::numeric))))
);


--
-- Name: v_balance_sheet; Type: VIEW; Schema: finance; Owner: -
--

CREATE VIEW finance.v_balance_sheet AS
 SELECT c.code,
    c.name,
    c.name_th,
    c.account_type,
    (
        CASE
            WHEN ((c.account_type)::text = ANY ((ARRAY['asset'::character varying, 'expense'::character varying])::text[])) THEN (COALESCE(sum(l.debit), (0)::numeric) - COALESCE(sum(l.credit), (0)::numeric))
            ELSE (COALESCE(sum(l.credit), (0)::numeric) - COALESCE(sum(l.debit), (0)::numeric))
        END)::double precision AS balance
   FROM ((folio.chart_of_accounts c
     LEFT JOIN folio.ledger_lines l ON (((l.account_code)::text = (c.code)::text)))
     LEFT JOIN folio.journal_entries j ON (((j.id = l.journal_entry_id) AND (j.is_draft = false))))
  WHERE ((c.account_type)::text = ANY ((ARRAY['asset'::character varying, 'liability'::character varying, 'equity'::character varying])::text[]))
  GROUP BY c.code, c.name, c.name_th, c.account_type;


--
-- Name: v_cashflow_classified; Type: VIEW; Schema: finance; Owner: -
--

CREATE VIEW finance.v_cashflow_classified AS
 SELECT l.id,
    j.id AS journal_entry_id,
    j.entry_date,
    j.is_draft,
    j.finalized_at,
    l.account_code,
    c.name AS account_name,
    c.account_type AS coa_account_type,
    COALESCE(m.activity, 'unclassified'::character varying) AS activity,
    COALESCE(m.is_cash_account, false) AS is_cash_account,
    l.debit,
    l.credit,
    l.description
   FROM (((folio.ledger_lines l
     JOIN folio.journal_entries j ON ((j.id = l.journal_entry_id)))
     JOIN folio.chart_of_accounts c ON (((c.code)::text = (l.account_code)::text)))
     LEFT JOIN finance.account_cf_class m ON ((((m.account_code)::text = (l.account_code)::text) AND ((j.entry_date >= m.effective_from) AND (j.entry_date <= m.effective_to)))))
  WHERE (j.is_draft = false);


--
-- Name: v_income_statement; Type: VIEW; Schema: finance; Owner: -
--

CREATE VIEW finance.v_income_statement AS
 SELECT c.code,
    c.name,
    c.name_th,
    c.account_type,
    (
        CASE
            WHEN ((c.account_type)::text = 'revenue'::text) THEN (COALESCE(sum(l.credit), (0)::numeric) - COALESCE(sum(l.debit), (0)::numeric))
            WHEN ((c.account_type)::text = 'expense'::text) THEN (COALESCE(sum(l.debit), (0)::numeric) - COALESCE(sum(l.credit), (0)::numeric))
            ELSE (0)::numeric
        END)::double precision AS amount
   FROM ((folio.chart_of_accounts c
     LEFT JOIN folio.ledger_lines l ON (((l.account_code)::text = (c.code)::text)))
     LEFT JOIN folio.journal_entries j ON (((j.id = l.journal_entry_id) AND (j.is_draft = false))))
  WHERE ((c.account_type)::text = ANY ((ARRAY['revenue'::character varying, 'expense'::character varying])::text[]))
  GROUP BY c.code, c.name, c.name_th, c.account_type;


--
-- Name: v_period_summary; Type: VIEW; Schema: finance; Owner: -
--

CREATE VIEW finance.v_period_summary AS
 SELECT j.id AS journal_entry_id,
    j.entry_date,
    j.description,
    j.expense_id,
    j.pr_id,
    j.po_id,
    j.so_id,
    j.step,
    (COALESCE(sum(l.debit), (0)::numeric))::double precision AS total_debit,
    (COALESCE(sum(l.credit), (0)::numeric))::double precision AS total_credit
   FROM (folio.journal_entries j
     LEFT JOIN folio.ledger_lines l ON ((l.journal_entry_id = j.id)))
  WHERE (j.is_draft = false)
  GROUP BY j.id, j.entry_date, j.description, j.expense_id, j.pr_id, j.po_id, j.so_id, j.step;


--
-- Name: v_trial_balance; Type: VIEW; Schema: finance; Owner: -
--

CREATE VIEW finance.v_trial_balance AS
 SELECT c.code,
    c.name,
    c.name_th,
    c.account_type,
    (COALESCE(sum(l.debit), (0)::numeric))::double precision AS period_debit,
    (COALESCE(sum(l.credit), (0)::numeric))::double precision AS period_credit,
    ((COALESCE(sum(l.debit), (0)::numeric))::double precision - (COALESCE(sum(l.credit), (0)::numeric))::double precision) AS net
   FROM ((folio.chart_of_accounts c
     LEFT JOIN folio.ledger_lines l ON (((l.account_code)::text = (c.code)::text)))
     LEFT JOIN folio.journal_entries j ON (((j.id = l.journal_entry_id) AND (j.is_draft = false))))
  GROUP BY c.code, c.name, c.name_th, c.account_type;


--
-- Name: access_requests; Type: TABLE; Schema: folio; Owner: -
--

CREATE TABLE folio.access_requests (
    id integer NOT NULL,
    actor_id integer NOT NULL,
    tile_id text NOT NULL,
    tile_title text,
    note text,
    status text DEFAULT 'pending'::text NOT NULL,
    target_user_id integer,
    target_role text,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    resolved_at timestamp with time zone,
    resolved_by_user_id integer,
    resolved_note text
);


--
-- Name: access_requests_id_seq; Type: SEQUENCE; Schema: folio; Owner: -
--

CREATE SEQUENCE folio.access_requests_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: access_requests_id_seq; Type: SEQUENCE OWNED BY; Schema: folio; Owner: -
--

ALTER SEQUENCE folio.access_requests_id_seq OWNED BY folio.access_requests.id;


--
-- Name: ai_assignments; Type: TABLE; Schema: folio; Owner: -
--

CREATE TABLE folio.ai_assignments (
    id integer NOT NULL,
    section_key text NOT NULL,
    task_type character varying(20) NOT NULL,
    provider_id integer,
    model_id integer,
    staff_id integer,
    params_json jsonb DEFAULT '{}'::jsonb NOT NULL,
    priority integer DEFAULT 100 NOT NULL,
    enabled boolean DEFAULT true NOT NULL,
    created_at timestamp without time zone DEFAULT now() NOT NULL,
    updated_at timestamp without time zone DEFAULT now() NOT NULL,
    CONSTRAINT ai_assignments_task_type_check CHECK (((task_type)::text = ANY (ARRAY[('embed'::character varying)::text, ('chat'::character varying)::text, ('vision'::character varying)::text])))
);


--
-- Name: ai_assignments_id_seq; Type: SEQUENCE; Schema: folio; Owner: -
--

CREATE SEQUENCE folio.ai_assignments_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: ai_assignments_id_seq; Type: SEQUENCE OWNED BY; Schema: folio; Owner: -
--

ALTER SEQUENCE folio.ai_assignments_id_seq OWNED BY folio.ai_assignments.id;


--
-- Name: ai_chat_sessions; Type: TABLE; Schema: folio; Owner: -
--

CREATE TABLE folio.ai_chat_sessions (
    id bigint NOT NULL,
    user_id integer NOT NULL,
    scope text NOT NULL,
    messages jsonb DEFAULT '[]'::jsonb NOT NULL,
    meta jsonb DEFAULT '{}'::jsonb NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: ai_chat_sessions_id_seq; Type: SEQUENCE; Schema: folio; Owner: -
--

CREATE SEQUENCE folio.ai_chat_sessions_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: ai_chat_sessions_id_seq; Type: SEQUENCE OWNED BY; Schema: folio; Owner: -
--

ALTER SEQUENCE folio.ai_chat_sessions_id_seq OWNED BY folio.ai_chat_sessions.id;


--
-- Name: ai_invocations; Type: TABLE; Schema: folio; Owner: -
--

CREATE TABLE folio.ai_invocations (
    id bigint NOT NULL,
    staff_id integer,
    section_key text,
    task_type character varying(20),
    provider_id integer,
    model_id integer,
    prompt_tokens integer,
    response_tokens integer,
    latency_ms integer,
    status character varying(20) NOT NULL,
    error text,
    prompt_excerpt text,
    response_excerpt text,
    actor_id integer,
    created_at timestamp without time zone DEFAULT now() NOT NULL
);


--
-- Name: ai_invocations_id_seq; Type: SEQUENCE; Schema: folio; Owner: -
--

CREATE SEQUENCE folio.ai_invocations_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: ai_invocations_id_seq; Type: SEQUENCE OWNED BY; Schema: folio; Owner: -
--

ALTER SEQUENCE folio.ai_invocations_id_seq OWNED BY folio.ai_invocations.id;


--
-- Name: ai_models; Type: TABLE; Schema: folio; Owner: -
--

CREATE TABLE folio.ai_models (
    id integer NOT NULL,
    provider_id integer NOT NULL,
    name text NOT NULL,
    capabilities text[] DEFAULT '{}'::text[] NOT NULL,
    context_window integer,
    defaults_json jsonb DEFAULT '{}'::jsonb NOT NULL,
    enabled boolean DEFAULT true NOT NULL,
    created_at timestamp without time zone DEFAULT now() NOT NULL,
    description text
);


--
-- Name: ai_models_id_seq; Type: SEQUENCE; Schema: folio; Owner: -
--

CREATE SEQUENCE folio.ai_models_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: ai_models_id_seq; Type: SEQUENCE OWNED BY; Schema: folio; Owner: -
--

ALTER SEQUENCE folio.ai_models_id_seq OWNED BY folio.ai_models.id;


--
-- Name: ai_providers; Type: TABLE; Schema: folio; Owner: -
--

CREATE TABLE folio.ai_providers (
    id integer NOT NULL,
    name text NOT NULL,
    type character varying(20) NOT NULL,
    base_url text NOT NULL,
    api_key_enc bytea,
    enabled boolean DEFAULT true NOT NULL,
    preset character varying(40),
    notes text,
    created_at timestamp without time zone DEFAULT now() NOT NULL,
    updated_at timestamp without time zone DEFAULT now() NOT NULL,
    CONSTRAINT ai_providers_type_check CHECK (((type)::text = ANY (ARRAY[('ollama'::character varying)::text, ('openai_compat'::character varying)::text, ('minimax'::character varying)::text])))
);


--
-- Name: ai_providers_id_seq; Type: SEQUENCE; Schema: folio; Owner: -
--

CREATE SEQUENCE folio.ai_providers_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: ai_providers_id_seq; Type: SEQUENCE OWNED BY; Schema: folio; Owner: -
--

ALTER SEQUENCE folio.ai_providers_id_seq OWNED BY folio.ai_providers.id;


--
-- Name: ai_section_health; Type: VIEW; Schema: folio; Owner: -
--

CREATE VIEW folio.ai_section_health AS
 SELECT a.id AS assignment_id,
    a.section_key,
    a.task_type,
    a.enabled AS assignment_enabled,
    a.priority,
    p.id AS provider_id,
    p.name AS provider_name,
    p.type AS provider_type,
    m.id AS model_id,
    m.name AS model_name,
    count(i.id) FILTER (WHERE ((i.status)::text = 'ok'::text)) AS ok_calls,
    count(i.id) FILTER (WHERE ((i.status)::text = 'error'::text)) AS err_calls,
    count(i.id) AS total_calls,
    max(i.created_at) AS last_invocation_at,
    min(i.created_at) AS first_invocation_at
   FROM (((folio.ai_assignments a
     LEFT JOIN folio.ai_providers p ON ((p.id = a.provider_id)))
     LEFT JOIN folio.ai_models m ON ((m.id = a.model_id)))
     LEFT JOIN folio.ai_invocations i ON ((i.section_key = a.section_key)))
  GROUP BY a.id, a.section_key, a.task_type, a.enabled, a.priority, p.id, p.name, p.type, m.id, m.name;


--
-- Name: ai_staff; Type: TABLE; Schema: folio; Owner: -
--

CREATE TABLE folio.ai_staff (
    id integer NOT NULL,
    name text NOT NULL,
    role_label text,
    description text,
    system_prompt text NOT NULL,
    capabilities text[] DEFAULT '{}'::text[] NOT NULL,
    default_provider_id integer,
    default_model_id integer,
    enabled boolean DEFAULT true NOT NULL,
    created_at timestamp without time zone DEFAULT now() NOT NULL,
    updated_at timestamp without time zone DEFAULT now() NOT NULL
);


--
-- Name: ai_staff_id_seq; Type: SEQUENCE; Schema: folio; Owner: -
--

CREATE SEQUENCE folio.ai_staff_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: ai_staff_id_seq; Type: SEQUENCE OWNED BY; Schema: folio; Owner: -
--

ALTER SEQUENCE folio.ai_staff_id_seq OWNED BY folio.ai_staff.id;


--
-- Name: approval_override_audit; Type: TABLE; Schema: folio; Owner: -
--

CREATE TABLE folio.approval_override_audit (
    id bigint NOT NULL,
    target_type character varying(20) NOT NULL,
    target_id integer,
    actor_id integer,
    kind character varying(20) NOT NULL,
    attempted_stage character varying(50),
    required_role character varying(50),
    actor_role character varying(50),
    reason text,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT approval_override_audit_kind_check CHECK (((kind)::text = ANY (ARRAY[('granted'::character varying)::text, ('denied'::character varying)::text]))),
    CONSTRAINT approval_override_audit_target_type_check CHECK (((target_type)::text = ANY (ARRAY[('expense'::character varying)::text, ('pr'::character varying)::text, ('po'::character varying)::text])))
);


--
-- Name: approval_override_audit_id_seq; Type: SEQUENCE; Schema: folio; Owner: -
--

CREATE SEQUENCE folio.approval_override_audit_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: approval_override_audit_id_seq; Type: SEQUENCE OWNED BY; Schema: folio; Owner: -
--

ALTER SEQUENCE folio.approval_override_audit_id_seq OWNED BY folio.approval_override_audit.id;


--
-- Name: approval_transitions; Type: TABLE; Schema: folio; Owner: -
--

CREATE TABLE folio.approval_transitions (
    id bigint NOT NULL,
    target_type character varying(20) NOT NULL,
    target_id integer NOT NULL,
    actor_id integer,
    previous_status character varying(50),
    new_status character varying(50),
    comments text,
    stage character varying(50),
    chain_index integer,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT approval_transitions_target_type_check CHECK (((target_type)::text = ANY (ARRAY[('expense'::character varying)::text, ('pr'::character varying)::text, ('po'::character varying)::text])))
);


--
-- Name: approval_transitions_id_seq; Type: SEQUENCE; Schema: folio; Owner: -
--

CREATE SEQUENCE folio.approval_transitions_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: approval_transitions_id_seq; Type: SEQUENCE OWNED BY; Schema: folio; Owner: -
--

ALTER SEQUENCE folio.approval_transitions_id_seq OWNED BY folio.approval_transitions.id;


--
-- Name: approver_nudges; Type: TABLE; Schema: folio; Owner: -
--

CREATE TABLE folio.approver_nudges (
    id bigint NOT NULL,
    approver_user_id integer NOT NULL,
    waybill_id text NOT NULL,
    stage text NOT NULL,
    hint text NOT NULL,
    sent_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: approver_nudges_id_seq; Type: SEQUENCE; Schema: folio; Owner: -
--

CREATE SEQUENCE folio.approver_nudges_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: approver_nudges_id_seq; Type: SEQUENCE OWNED BY; Schema: folio; Owner: -
--

ALTER SEQUENCE folio.approver_nudges_id_seq OWNED BY folio.approver_nudges.id;


--
-- Name: customer_advisories; Type: TABLE; Schema: folio; Owner: -
--

CREATE TABLE folio.customer_advisories (
    customer_id integer NOT NULL,
    advisory text NOT NULL,
    severity text NOT NULL,
    computed_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT customer_advisories_severity_check CHECK ((severity = ANY (ARRAY['ok'::text, 'watch'::text, 'critical'::text])))
);


--
-- Name: customers; Type: TABLE; Schema: folio; Owner: -
--

CREATE TABLE folio.customers (
    id integer NOT NULL,
    code text NOT NULL,
    name text NOT NULL,
    name_th text,
    tax_id text,
    billing_address text,
    shipping_address text,
    contact_name text,
    contact_email text,
    contact_phone text,
    credit_limit_thb numeric(14,2) DEFAULT 0 NOT NULL,
    payment_terms text DEFAULT 'Net 30'::text NOT NULL,
    blacklist boolean DEFAULT false NOT NULL,
    is_active boolean DEFAULT true NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    name_de text,
    embedding public.vector(1024)
);


--
-- Name: sales_orders; Type: TABLE; Schema: folio; Owner: -
--

CREATE TABLE folio.sales_orders (
    id integer NOT NULL,
    so_number text NOT NULL,
    customer_id integer NOT NULL,
    sales_rep_id integer NOT NULL,
    status text DEFAULT 'so_draft'::text NOT NULL,
    payment_terms text DEFAULT 'Net 30'::text NOT NULL,
    due_date date,
    invoice_number text,
    invoice_issued_at timestamp with time zone,
    ar_slip_id integer,
    vat_account_code text DEFAULT '210300'::text NOT NULL,
    ar_account_code text DEFAULT '110400'::text NOT NULL,
    cash_account_code text DEFAULT '110200'::text NOT NULL,
    revenue_account_code text DEFAULT '410100'::text NOT NULL,
    subtotal numeric(14,2) DEFAULT 0 NOT NULL,
    vat_total numeric(14,2) DEFAULT 0 NOT NULL,
    total_amount numeric(14,2) DEFAULT 0 NOT NULL,
    currency text DEFAULT 'THB'::text NOT NULL,
    rejection_reason text,
    rejection_actor_id integer,
    rejected_at timestamp with time zone,
    invoice_issuer_id integer,
    paid_by_id integer,
    paid_at timestamp with time zone,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT sales_orders_status_check CHECK ((status = ANY (ARRAY['so_draft'::text, 'so_sales_review'::text, 'so_dept_approval'::text, 'so_credit_check'::text, 'so_invoiced'::text, 'so_paid'::text, 'rejected'::text])))
);


--
-- Name: customer_ar_history; Type: VIEW; Schema: folio; Owner: -
--

CREATE VIEW folio.customer_ar_history AS
 SELECT c.id AS customer_id,
    c.code AS customer_code,
    c.name AS customer_name,
    c.credit_limit_thb AS credit_limit,
    (COALESCE(sum(so.total_amount), (0)::numeric))::numeric(14,2) AS total_invoiced,
    (COALESCE(sum(
        CASE
            WHEN (so.status <> ALL (ARRAY['so_paid'::text, 'rejected'::text])) THEN so.total_amount
            ELSE (0)::numeric
        END), (0)::numeric))::numeric(14,2) AS outstanding_ar,
    (COALESCE(sum(
        CASE
            WHEN (so.status = 'so_paid'::text) THEN so.total_amount
            ELSE (0)::numeric
        END), (0)::numeric))::numeric(14,2) AS total_paid,
    (count(so.id))::integer AS so_count
   FROM (folio.customers c
     LEFT JOIN folio.sales_orders so ON ((so.customer_id = c.id)))
  GROUP BY c.id, c.code, c.name, c.credit_limit_thb;


--
-- Name: customer_contacts; Type: TABLE; Schema: folio; Owner: -
--

CREATE TABLE folio.customer_contacts (
    id integer NOT NULL,
    customer_id integer NOT NULL,
    fullname text NOT NULL,
    role text,
    email text,
    phone text,
    notes text,
    created_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: customer_contacts_id_seq; Type: SEQUENCE; Schema: folio; Owner: -
--

CREATE SEQUENCE folio.customer_contacts_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: customer_contacts_id_seq; Type: SEQUENCE OWNED BY; Schema: folio; Owner: -
--

ALTER SEQUENCE folio.customer_contacts_id_seq OWNED BY folio.customer_contacts.id;


--
-- Name: customers_id_seq; Type: SEQUENCE; Schema: folio; Owner: -
--

CREATE SEQUENCE folio.customers_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: customers_id_seq; Type: SEQUENCE OWNED BY; Schema: folio; Owner: -
--

ALTER SEQUENCE folio.customers_id_seq OWNED BY folio.customers.id;


--
-- Name: domain_events; Type: TABLE; Schema: folio; Owner: -
--

CREATE TABLE folio.domain_events (
    id bigint NOT NULL,
    type character varying(80) NOT NULL,
    actor_id integer,
    ref_type character varying(40),
    ref_id bigint,
    payload jsonb,
    severity character varying(20) DEFAULT 'info'::character varying,
    created_at timestamp without time zone DEFAULT CURRENT_TIMESTAMP NOT NULL
);


--
-- Name: domain_events_id_seq; Type: SEQUENCE; Schema: folio; Owner: -
--

CREATE SEQUENCE folio.domain_events_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: domain_events_id_seq; Type: SEQUENCE OWNED BY; Schema: folio; Owner: -
--

ALTER SEQUENCE folio.domain_events_id_seq OWNED BY folio.domain_events.id;


--
-- Name: exec_snapshots; Type: TABLE; Schema: folio; Owner: -
--

CREATE TABLE folio.exec_snapshots (
    id bigint NOT NULL,
    snapshot_date date NOT NULL,
    kpis jsonb NOT NULL,
    dept_budgets jsonb DEFAULT '[]'::jsonb NOT NULL,
    stuck_count integer DEFAULT 0 NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: exec_snapshots_id_seq; Type: SEQUENCE; Schema: folio; Owner: -
--

CREATE SEQUENCE folio.exec_snapshots_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: exec_snapshots_id_seq; Type: SEQUENCE OWNED BY; Schema: folio; Owner: -
--

ALTER SEQUENCE folio.exec_snapshots_id_seq OWNED BY folio.exec_snapshots.id;


--
-- Name: expense_items; Type: TABLE; Schema: folio; Owner: -
--

CREATE TABLE folio.expense_items (
    id integer NOT NULL,
    expense_id integer,
    description character varying(255) NOT NULL,
    amount numeric(12,2) NOT NULL,
    mapped_account_code character varying(20),
    confidence_score double precision,
    created_at timestamp without time zone DEFAULT CURRENT_TIMESTAMP,
    qty numeric(12,2) DEFAULT 1.00 NOT NULL,
    unit_price numeric(12,2) DEFAULT 0.00 NOT NULL
);


--
-- Name: expense_items_id_seq; Type: SEQUENCE; Schema: folio; Owner: -
--

CREATE SEQUENCE folio.expense_items_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: expense_items_id_seq; Type: SEQUENCE OWNED BY; Schema: folio; Owner: -
--

ALTER SEQUENCE folio.expense_items_id_seq OWNED BY folio.expense_items.id;


--
-- Name: expense_payments; Type: TABLE; Schema: folio; Owner: -
--

CREATE TABLE folio.expense_payments (
    id bigint NOT NULL,
    waybill_id text NOT NULL,
    expense_id integer NOT NULL,
    slip_id integer NOT NULL,
    amount numeric(14,2) NOT NULL,
    payment_date date NOT NULL,
    bank_name text NOT NULL,
    account_number text,
    payee text NOT NULL,
    reference text NOT NULL,
    ocr_payload jsonb,
    ocr_confidence numeric(5,4),
    confirmed_by integer NOT NULL,
    confirmed_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT expense_payments_amount_check CHECK ((amount > (0)::numeric))
);


--
-- Name: expense_payments_id_seq; Type: SEQUENCE; Schema: folio; Owner: -
--

CREATE SEQUENCE folio.expense_payments_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: expense_payments_id_seq; Type: SEQUENCE OWNED BY; Schema: folio; Owner: -
--

ALTER SEQUENCE folio.expense_payments_id_seq OWNED BY folio.expense_payments.id;


--
-- Name: expenses; Type: TABLE; Schema: folio; Owner: -
--

CREATE TABLE folio.expenses (
    id integer NOT NULL,
    submitter_id integer,
    vendor_name character varying(150),
    transaction_date date,
    subtotal numeric(12,2) DEFAULT 0.00,
    vat_amount numeric(12,2) DEFAULT 0.00,
    total_amount numeric(12,2) DEFAULT 0.00,
    payment_method character varying(50),
    status character varying(50) DEFAULT 'draft'::character varying,
    ocr_raw_json jsonb,
    is_corrupted boolean DEFAULT false,
    correction_notes text,
    document_url character varying(255),
    created_at timestamp without time zone DEFAULT CURRENT_TIMESTAMP,
    updated_at timestamp without time zone DEFAULT CURRENT_TIMESTAMP,
    rejection_reason text,
    rejection_actor_id integer,
    rejected_at timestamp without time zone,
    disbursed_at timestamp without time zone,
    disbursed_by integer,
    gl_confirmed_at timestamp without time zone,
    gl_confirmed_by integer,
    pr_id integer,
    po_id integer,
    journal_entry_id integer,
    created_to character varying(150),
    vendor_address text,
    created_to_address text,
    payee_type text DEFAULT 'employee'::text NOT NULL,
    CONSTRAINT expenses_payee_type_chk CHECK ((payee_type = ANY (ARRAY['employee'::text, 'vendor'::text]))),
    CONSTRAINT expenses_status_chk CHECK (((status)::text = ANY ((ARRAY['draft'::character varying, 'submission'::character varying, 'department_approval'::character varying, 'accounting_review'::character varying, 'accounting_approval'::character varying, 'executive_approval'::character varying, 'payment'::character varying, 'settlement'::character varying, 'completed'::character varying, 'rejected'::character varying])::text[])))
);


--
-- Name: expenses_id_seq; Type: SEQUENCE; Schema: folio; Owner: -
--

CREATE SEQUENCE folio.expenses_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: expenses_id_seq; Type: SEQUENCE OWNED BY; Schema: folio; Owner: -
--

ALTER SEQUENCE folio.expenses_id_seq OWNED BY folio.expenses.id;


--
-- Name: hook_events_id_seq; Type: SEQUENCE; Schema: folio; Owner: -
--

CREATE SEQUENCE folio.hook_events_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: hr_leave; Type: TABLE; Schema: folio; Owner: -
--

CREATE TABLE folio.hr_leave (
    waybill_id text NOT NULL,
    employee_id integer NOT NULL,
    leave_type text NOT NULL,
    start_date date NOT NULL,
    end_date date NOT NULL,
    days numeric(3,1) NOT NULL,
    reason text,
    medical_cert_note text,
    CONSTRAINT hr_leave_days_check CHECK ((days > (0)::numeric)),
    CONSTRAINT hr_leave_leave_type_check CHECK ((leave_type = ANY (ARRAY['sick'::text, 'annual'::text, 'personal'::text])))
);


--
-- Name: journal_entries_id_seq; Type: SEQUENCE; Schema: folio; Owner: -
--

CREATE SEQUENCE folio.journal_entries_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: journal_entries_id_seq; Type: SEQUENCE OWNED BY; Schema: folio; Owner: -
--

ALTER SEQUENCE folio.journal_entries_id_seq OWNED BY folio.journal_entries.id;


--
-- Name: learned_mappings; Type: TABLE; Schema: folio; Owner: -
--

CREATE TABLE folio.learned_mappings (
    vendor_name_norm text NOT NULL,
    account_code text NOT NULL,
    hits integer DEFAULT 1 NOT NULL,
    last_used_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: ledger_lines_id_seq; Type: SEQUENCE; Schema: folio; Owner: -
--

CREATE SEQUENCE folio.ledger_lines_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: ledger_lines_id_seq; Type: SEQUENCE OWNED BY; Schema: folio; Owner: -
--

ALTER SEQUENCE folio.ledger_lines_id_seq OWNED BY folio.ledger_lines.id;


--
-- Name: notification_digests; Type: TABLE; Schema: folio; Owner: -
--

CREATE TABLE folio.notification_digests (
    id bigint NOT NULL,
    user_id integer NOT NULL,
    period_start timestamp with time zone NOT NULL,
    period_end timestamp with time zone NOT NULL,
    severity text NOT NULL,
    bullets jsonb NOT NULL,
    source_count integer NOT NULL,
    generated_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: notification_digests_id_seq; Type: SEQUENCE; Schema: folio; Owner: -
--

CREATE SEQUENCE folio.notification_digests_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: notification_digests_id_seq; Type: SEQUENCE OWNED BY; Schema: folio; Owner: -
--

ALTER SEQUENCE folio.notification_digests_id_seq OWNED BY folio.notification_digests.id;


--
-- Name: notifications; Type: TABLE; Schema: folio; Owner: -
--

CREATE TABLE folio.notifications (
    id integer NOT NULL,
    user_id integer NOT NULL,
    type character varying(50) NOT NULL,
    target_type character varying(20),
    target_id integer,
    waybill_id text,
    event_id uuid,
    category text DEFAULT 'update'::text NOT NULL,
    audience text DEFAULT 'owner'::text NOT NULL,
    stage_key text,
    message_key text,
    payload_json jsonb,
    severity text DEFAULT 'info'::text NOT NULL,
    href text,
    read_at timestamp without time zone,
    created_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP NOT NULL,
    resolved_at timestamp with time zone,
    resolved_by integer,
    first_opened_at timestamp with time zone,
    last_opened_at timestamp with time zone,
    open_count integer DEFAULT 0 NOT NULL,
    CONSTRAINT notifications_category_check CHECK ((category = ANY (ARRAY['action'::text, 'update'::text]))),
    CONSTRAINT notifications_audience_check CHECK ((audience = ANY (ARRAY['owner'::text, 'approver'::text, 'watcher'::text]))),
    CONSTRAINT notifications_severity_check CHECK ((severity = ANY (ARRAY['info'::text, 'success'::text, 'warning'::text, 'error'::text]))),
    CONSTRAINT notifications_open_count_check CHECK ((open_count >= 0))
);


--
-- Name: notifications_id_seq; Type: SEQUENCE; Schema: folio; Owner: -
--

CREATE SEQUENCE folio.notifications_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: notifications_id_seq; Type: SEQUENCE OWNED BY; Schema: folio; Owner: -
--

ALTER SEQUENCE folio.notifications_id_seq OWNED BY folio.notifications.id;


--
-- Name: po_invoices; Type: TABLE; Schema: folio; Owner: -
--

CREATE TABLE folio.po_invoices (
    id bigint NOT NULL,
    vendor_name text,
    vendor_id integer,
    invoice_no text,
    invoice_date date,
    file_path text NOT NULL,
    mime_type text,
    file_size bigint,
    status text DEFAULT 'pending'::text NOT NULL,
    draft_pr_id integer,
    draft_po_id integer,
    extracted jsonb,
    error text,
    uploaded_by integer,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT po_invoices_status_check CHECK ((status = ANY (ARRAY['pending'::text, 'extracted'::text, 'drafted'::text, 'rejected'::text, 'failed'::text])))
);


--
-- Name: po_invoices_id_seq; Type: SEQUENCE; Schema: folio; Owner: -
--

CREATE SEQUENCE folio.po_invoices_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: po_invoices_id_seq; Type: SEQUENCE OWNED BY; Schema: folio; Owner: -
--

ALTER SEQUENCE folio.po_invoices_id_seq OWNED BY folio.po_invoices.id;


--
-- Name: po_items; Type: TABLE; Schema: folio; Owner: -
--

CREATE TABLE folio.po_items (
    id integer NOT NULL,
    po_id integer NOT NULL,
    description character varying(255) NOT NULL,
    qty numeric(12,2) DEFAULT 1,
    unit_price numeric(12,2) DEFAULT 0,
    mapped_account_code character varying(20),
    created_at timestamp without time zone DEFAULT CURRENT_TIMESTAMP
);


--
-- Name: po_items_id_seq; Type: SEQUENCE; Schema: folio; Owner: -
--

CREATE SEQUENCE folio.po_items_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: po_items_id_seq; Type: SEQUENCE OWNED BY; Schema: folio; Owner: -
--

ALTER SEQUENCE folio.po_items_id_seq OWNED BY folio.po_items.id;


--
-- Name: policy_audit; Type: TABLE; Schema: folio; Owner: -
--

CREATE TABLE folio.policy_audit (
    id integer NOT NULL,
    policy_id integer,
    actor_id integer,
    before_json jsonb,
    after_json jsonb,
    changed_at timestamp without time zone DEFAULT CURRENT_TIMESTAMP
);


--
-- Name: policy_audit_id_seq; Type: SEQUENCE; Schema: folio; Owner: -
--

CREATE SEQUENCE folio.policy_audit_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: policy_audit_id_seq; Type: SEQUENCE OWNED BY; Schema: folio; Owner: -
--

ALTER SEQUENCE folio.policy_audit_id_seq OWNED BY folio.policy_audit.id;


--
-- Name: policy_lint_results; Type: TABLE; Schema: folio; Owner: -
--

CREATE TABLE folio.policy_lint_results (
    policy_id text NOT NULL,
    findings jsonb DEFAULT '[]'::jsonb NOT NULL,
    generated_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: pr_items; Type: TABLE; Schema: folio; Owner: -
--

CREATE TABLE folio.pr_items (
    id integer NOT NULL,
    pr_id integer,
    description character varying(255) NOT NULL,
    qty numeric(12,2) DEFAULT 1.00 NOT NULL,
    unit_price numeric(12,2) DEFAULT 0.00 NOT NULL,
    mapped_account_code character varying(20),
    confidence_score double precision,
    created_at timestamp without time zone DEFAULT CURRENT_TIMESTAMP
);


--
-- Name: pr_items_id_seq; Type: SEQUENCE; Schema: folio; Owner: -
--

CREATE SEQUENCE folio.pr_items_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: pr_items_id_seq; Type: SEQUENCE OWNED BY; Schema: folio; Owner: -
--

ALTER SEQUENCE folio.pr_items_id_seq OWNED BY folio.pr_items.id;


--
-- Name: purchase_orders; Type: TABLE; Schema: folio; Owner: -
--

CREATE TABLE folio.purchase_orders (
    id integer NOT NULL,
    pr_id integer NOT NULL,
    po_number character varying(40) NOT NULL,
    vendor_name character varying(150),
    total_amount numeric(14,2) DEFAULT 0,
    currency character varying(10) DEFAULT 'THB'::character varying,
    status character varying(50) DEFAULT 'draft'::character varying,
    issued_at timestamp without time zone DEFAULT CURRENT_TIMESTAMP,
    issued_by integer,
    settled_at timestamp without time zone,
    settled_by integer,
    settled_slip_id integer,
    rejection_reason text,
    rejection_actor_id integer,
    rejected_at timestamp without time zone,
    created_at timestamp without time zone DEFAULT CURRENT_TIMESTAMP,
    updated_at timestamp without time zone DEFAULT CURRENT_TIMESTAMP,
    vendor_country character(2),
    CONSTRAINT purchase_orders_status_chk CHECK (((status)::text = ANY ((ARRAY['draft'::character varying, 'submission'::character varying, 'dept_verification'::character varying, 'dept_authorization'::character varying, 'accounting_verification'::character varying, 'accounting_supervision'::character varying, 'accounting_authorization'::character varying, 'disbursement_authorization'::character varying, 'cfo_authorization'::character varying, 'ceo_authorization'::character varying, 'awaiting_disbursement'::character varying, 'disbursed'::character varying, 'rejected'::character varying])::text[])))
);


--
-- Name: purchase_orders_id_seq; Type: SEQUENCE; Schema: folio; Owner: -
--

CREATE SEQUENCE folio.purchase_orders_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: purchase_orders_id_seq; Type: SEQUENCE OWNED BY; Schema: folio; Owner: -
--

ALTER SEQUENCE folio.purchase_orders_id_seq OWNED BY folio.purchase_orders.id;


--
-- Name: purchase_requisitions; Type: TABLE; Schema: folio; Owner: -
--

CREATE TABLE folio.purchase_requisitions (
    id integer NOT NULL,
    requester_id integer,
    vendor_name character varying(150),
    need_by_date date,
    status character varying(50) DEFAULT 'draft'::character varying,
    total_estimate numeric(14,2) DEFAULT 0.00,
    currency character varying(10) DEFAULT 'THB'::character varying,
    justification text,
    document_url character varying(500),
    is_recurring boolean DEFAULT false,
    created_at timestamp without time zone DEFAULT CURRENT_TIMESTAMP,
    updated_at timestamp without time zone DEFAULT CURRENT_TIMESTAMP,
    rejection_reason text,
    rejection_actor_id integer,
    rejected_at timestamp without time zone,
    dept_group_id text,
    pr_number text GENERATED ALWAYS AS (((('PR-'::text || (EXTRACT(year FROM created_at))::text) || '-'::text) || lpad((id)::text, 6, '0'::text))) STORED,
    vendor_country character(2),
    CONSTRAINT purchase_requisitions_status_chk CHECK (((status)::text = ANY ((ARRAY['draft'::character varying, 'submission'::character varying, 'dept_verification'::character varying, 'dept_authorization'::character varying, 'accounting_verification'::character varying, 'accounting_supervision'::character varying, 'accounting_authorization'::character varying, 'disbursement_authorization'::character varying, 'cfo_authorization'::character varying, 'ceo_authorization'::character varying, 'awaiting_disbursement'::character varying, 'disbursed'::character varying, 'rejected'::character varying])::text[])))
);


--
-- Name: purchase_requisitions_id_seq; Type: SEQUENCE; Schema: folio; Owner: -
--

CREATE SEQUENCE folio.purchase_requisitions_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: purchase_requisitions_id_seq; Type: SEQUENCE OWNED BY; Schema: folio; Owner: -
--

ALTER SEQUENCE folio.purchase_requisitions_id_seq OWNED BY folio.purchase_requisitions.id;


--
-- Name: sales_orders_fy_2026_seq; Type: SEQUENCE; Schema: folio; Owner: -
--

CREATE SEQUENCE folio.sales_orders_fy_2026_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: sales_orders_id_seq; Type: SEQUENCE; Schema: folio; Owner: -
--

CREATE SEQUENCE folio.sales_orders_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: sales_orders_id_seq; Type: SEQUENCE OWNED BY; Schema: folio; Owner: -
--

ALTER SEQUENCE folio.sales_orders_id_seq OWNED BY folio.sales_orders.id;


--
-- Name: sales_product_embeddings; Type: TABLE; Schema: folio; Owner: -
--

CREATE TABLE folio.sales_product_embeddings (
    id bigint NOT NULL,
    so_item_id integer NOT NULL,
    description text NOT NULL,
    embedding public.vector(1024),
    created_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: sales_product_embeddings_id_seq; Type: SEQUENCE; Schema: folio; Owner: -
--

CREATE SEQUENCE folio.sales_product_embeddings_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: sales_product_embeddings_id_seq; Type: SEQUENCE OWNED BY; Schema: folio; Owner: -
--

ALTER SEQUENCE folio.sales_product_embeddings_id_seq OWNED BY folio.sales_product_embeddings.id;


--
-- Name: slips; Type: TABLE; Schema: folio; Owner: -
--

CREATE TABLE folio.slips (
    id integer NOT NULL,
    expense_id integer,
    pr_id integer,
    file_path character varying(500) NOT NULL,
    mime_type character varying(100) NOT NULL,
    file_size integer NOT NULL,
    ocr_raw_json jsonb,
    ocr_confidence double precision,
    ai_reasoning text,
    uploaded_by integer,
    uploaded_at timestamp without time zone DEFAULT CURRENT_TIMESTAMP,
    po_id integer,
    status character varying(20) DEFAULT 'pending'::character varying NOT NULL,
    confirmed_at timestamp with time zone,
    discarded_at timestamp with time zone,
    discarded_by integer,
    kind character varying(20) DEFAULT 'receipt'::character varying NOT NULL,
    bank_name character varying(150),
    account_number character varying(30),
    account_name character varying(150),
    bank_branch character varying(150),
    CONSTRAINT slips_kind_chk CHECK (((kind)::text = ANY ((ARRAY['receipt'::character varying, 'book_bank'::character varying, 'payment_slip'::character varying])::text[])))
);


--
-- Name: COLUMN slips.status; Type: COMMENT; Schema: folio; Owner: -
--

COMMENT ON COLUMN folio.slips.status IS 'pending = uploaded + OCR done, no parent, uploader can still discard. confirmed = linked to expense/pr/po, in approval queue. Removal of a confirmed slip is blocked once the linked parent has any approved/rejected transition.';


--
-- Name: COLUMN slips.confirmed_at; Type: COMMENT; Schema: folio; Owner: -
--

COMMENT ON COLUMN folio.slips.confirmed_at IS 'Timestamp the slip was linked to its parent expense/pr/po.';


--
-- Name: COLUMN slips.discarded_at; Type: COMMENT; Schema: folio; Owner: -
--

COMMENT ON COLUMN folio.slips.discarded_at IS 'Timestamp the slip was removed by the uploader (null = not discarded).';


--
-- Name: COLUMN slips.discarded_by; Type: COMMENT; Schema: folio; Owner: -
--

COMMENT ON COLUMN folio.slips.discarded_by IS 'User who discarded the slip (null = not discarded).';


--
-- Name: COLUMN slips.kind; Type: COMMENT; Schema: folio; Owner: -
--

COMMENT ON COLUMN folio.slips.kind IS 'receipt = the typical receipt/ใบเสร็จ slip (default). book_bank = passbook image for a transfer payee — carries bank_name / account_number / account_name. Two slips per expense are permitted; each row has exactly one parent per the slips_exactly_one_parent trigger.';


--
-- Name: COLUMN slips.bank_name; Type: COMMENT; Schema: folio; Owner: -
--

COMMENT ON COLUMN folio.slips.bank_name IS 'For book_bank slips: issuing bank name (free text or one of Krungthai/SCB/Bangkok Bank/Kasikorn/TMBThanachai/Other).';


--
-- Name: COLUMN slips.account_number; Type: COMMENT; Schema: folio; Owner: -
--

COMMENT ON COLUMN folio.slips.account_number IS 'For book_bank slips: payee bank account number (digits, no dashes).';


--
-- Name: COLUMN slips.account_name; Type: COMMENT; Schema: folio; Owner: -
--

COMMENT ON COLUMN folio.slips.account_name IS 'For book_bank slips: payee name as printed on the passbook.';


--
-- Name: COLUMN slips.bank_branch; Type: COMMENT; Schema: folio; Owner: -
--

COMMENT ON COLUMN folio.slips.bank_branch IS 'For book_bank slips: branch as printed on the passbook, e.g. ''0080 สาขาฟิวเจอร์พาร์ค รังสิต''. Free text. Optional.';


--
-- Name: slips_id_seq; Type: SEQUENCE; Schema: folio; Owner: -
--

CREATE SEQUENCE folio.slips_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: slips_id_seq; Type: SEQUENCE OWNED BY; Schema: folio; Owner: -
--

ALTER SEQUENCE folio.slips_id_seq OWNED BY folio.slips.id;


--
-- Name: so_items; Type: TABLE; Schema: folio; Owner: -
--

CREATE TABLE folio.so_items (
    id integer NOT NULL,
    sales_order_id integer NOT NULL,
    description text NOT NULL,
    qty numeric(12,2) DEFAULT 1 NOT NULL,
    unit_price numeric(14,2) DEFAULT 0 NOT NULL,
    vat_amount numeric(14,2) DEFAULT 0 NOT NULL,
    line_total numeric(14,2) DEFAULT 0 NOT NULL,
    mapped_revenue_account_code text,
    confidence_score numeric(5,3),
    created_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: so_items_id_seq; Type: SEQUENCE; Schema: folio; Owner: -
--

CREATE SEQUENCE folio.so_items_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: so_items_id_seq; Type: SEQUENCE OWNED BY; Schema: folio; Owner: -
--

ALTER SEQUENCE folio.so_items_id_seq OWNED BY folio.so_items.id;


--
-- Name: users; Type: TABLE; Schema: folio; Owner: -
--

CREATE TABLE folio.users (
    id integer NOT NULL,
    employee_code character varying(20) NOT NULL,
    fullname character varying(100) NOT NULL,
    line_user_id character varying(100),
    created_at timestamp without time zone DEFAULT CURRENT_TIMESTAMP,
    is_active boolean DEFAULT true,
    hired_at date NOT NULL,
    secondary_locale text DEFAULT 'th'::text NOT NULL,
    "position" text DEFAULT ''::text NOT NULL,
    job_description text DEFAULT ''::text NOT NULL,
    quota_sick integer DEFAULT 30 NOT NULL,
    used_sick integer DEFAULT 0 NOT NULL,
    quota_annual integer DEFAULT 10 NOT NULL,
    used_annual integer DEFAULT 0 NOT NULL,
    quota_personal integer DEFAULT 6 NOT NULL,
    used_personal integer DEFAULT 0 NOT NULL,
    dept_label text,
    CONSTRAINT users_secondary_locale_check CHECK ((secondary_locale = ANY (ARRAY['th'::text, 'de'::text])))
);


--
-- Name: users_id_seq; Type: SEQUENCE; Schema: folio; Owner: -
--

CREATE SEQUENCE folio.users_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: users_id_seq; Type: SEQUENCE OWNED BY; Schema: folio; Owner: -
--

ALTER SEQUENCE folio.users_id_seq OWNED BY folio.users.id;


--
-- Name: vendor_embeddings; Type: TABLE; Schema: folio; Owner: -
--

CREATE TABLE folio.vendor_embeddings (
    id bigint NOT NULL,
    expense_id integer NOT NULL,
    submitter_id integer,
    vendor_name text,
    description text,
    amount_thb numeric(14,2),
    transaction_date date,
    embedding public.vector(1024),
    created_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: vendor_embeddings_id_seq; Type: SEQUENCE; Schema: folio; Owner: -
--

CREATE SEQUENCE folio.vendor_embeddings_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: vendor_embeddings_id_seq; Type: SEQUENCE OWNED BY; Schema: folio; Owner: -
--

ALTER SEQUENCE folio.vendor_embeddings_id_seq OWNED BY folio.vendor_embeddings.id;


--
-- Name: vision_chain; Type: TABLE; Schema: folio; Owner: -
--

CREATE TABLE folio.vision_chain (
    section_key text NOT NULL,
    models text[] NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: waybill_attachments; Type: TABLE; Schema: folio; Owner: -
--

CREATE TABLE folio.waybill_attachments (
    id bigint NOT NULL,
    waybill_id text NOT NULL,
    stage_key text NOT NULL,
    kind text NOT NULL,
    storage_backend text DEFAULT 'minio'::text NOT NULL,
    storage_key text NOT NULL,
    filename text NOT NULL,
    content_type text NOT NULL,
    byte_size integer NOT NULL,
    uploaded_by integer NOT NULL,
    uploaded_role text NOT NULL,
    caption text,
    occurred_at timestamp with time zone DEFAULT now() NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT waybill_attachments_byte_size_check CHECK ((byte_size >= 0)),
    CONSTRAINT waybill_attachments_kind_check CHECK ((kind = ANY (ARRAY['slip'::text, 'pr_doc'::text, 'po_doc'::text, 'expense_voucher'::text, 'payment_slip'::text, 'payment_receipt'::text, 'signoff_memo'::text, 'invoice'::text, 'wht_cert'::text, 'photo'::text, 'memo'::text, 'other'::text]))),
    CONSTRAINT waybill_attachments_storage_backend_check CHECK ((storage_backend = 'minio'::text))
);


--
-- Name: waybill_attachments_id_seq; Type: SEQUENCE; Schema: folio; Owner: -
--

CREATE SEQUENCE folio.waybill_attachments_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: waybill_attachments_id_seq; Type: SEQUENCE OWNED BY; Schema: folio; Owner: -
--

ALTER SEQUENCE folio.waybill_attachments_id_seq OWNED BY folio.waybill_attachments.id;


--
-- Name: waybill_events; Type: TABLE; Schema: folio; Owner: -
--

CREATE TABLE folio.waybill_events (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    waybill_id text NOT NULL,
    sequence integer NOT NULL,
    previous_event_id uuid,
    kind text NOT NULL,
    stage_from text,
    stage_to text,
    actor_id integer,
    actor_role text,
    actor_signature bytea,
    occurred_at timestamp with time zone DEFAULT now() NOT NULL,
    payload jsonb,
    CONSTRAINT waybill_events_kind_check CHECK ((kind = ANY (ARRAY['submitted'::text, 'advanced'::text, 'rejected'::text, 'corrected'::text, 'settled'::text, 'posted-to-gl'::text, 'slip-attached'::text, 'signed-off'::text, 'reversed'::text, 'authorization-overridden'::text, 'resubmitted'::text, 'superseded'::text, 'created'::text, 'attached'::text, 'gl-confirmed'::text, 'so-submitted'::text, 'so-reviewed'::text, 'so-credit-checked'::text, 'so-auto-approved'::text, 'so-invoiced'::text, 'so-rejected'::text, 'so-paid'::text, 'so-dept-approved'::text, 'posted-to-gl-sales-accrual'::text, 'posted-to-gl-sales-vat'::text, 'posted-to-gl-sales-settlement'::text, 'gl-confirmed-accrual'::text, 'coa-applied'::text, 'gl-confirmed-settlement'::text, 'gl-confirmed-sales-vat'::text, 'gl-confirmed-sales-accrual'::text, 'gl-confirmed-sales-settlement'::text, 'stage-claimed'::text, 'stage-released'::text, 'stage-reassigned'::text, 'executive-skipped'::text, 'payment-confirmed'::text, 'posted-to-gl-accrual'::text, 'posted-to-gl-settlement'::text]))),
    CONSTRAINT waybill_events_sequence_check CHECK ((sequence >= 1))
);


--
-- Name: waybill_reviews; Type: TABLE; Schema: folio; Owner: -
--

CREATE TABLE folio.waybill_reviews (
    waybill_id text NOT NULL,
    stage text NOT NULL,
    hint text NOT NULL,
    generated_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT waybill_reviews_stage_check CHECK ((stage = ANY (ARRAY['hod'::text, 'am'::text])))
);


--
-- Name: waybills; Type: TABLE; Schema: folio; Owner: -
--

CREATE TABLE folio.waybills (
    id text NOT NULL,
    origin text NOT NULL,
    origin_id integer NOT NULL,
    fiscal_year smallint NOT NULL,
    waybill_kind text NOT NULL,
    submitter_id integer,
    vendor_name text,
    total_amount numeric(14,2),
    currency text DEFAULT 'THB'::text NOT NULL,
    current_stage text NOT NULL,
    current_owner_role text,
    current_owner_user_id integer,
    status text DEFAULT 'open'::text NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    created_to text,
    vendor_address text,
    created_to_address text,
    flagged_reason jsonb,
    CONSTRAINT waybills_origin_check CHECK ((origin = ANY (ARRAY['expense'::text, 'pr'::text, 'po'::text, 'so'::text, 'hr_leave'::text]))),
    CONSTRAINT waybills_status_check CHECK ((status = ANY (ARRAY['open'::text, 'completed'::text, 'rejected'::text, 'reversed'::text, 'superseded'::text]))),
    CONSTRAINT waybills_waybill_kind_check CHECK ((waybill_kind = ANY (ARRAY['reimbursement'::text, 'procurement'::text, 'sales'::text, 'hr_leave'::text])))
);


--
-- Name: waybill_risk; Type: VIEW; Schema: folio; Owner: -
--

CREATE VIEW folio.waybill_risk AS
 SELECT id,
    folio.waybill_risk_score(id) AS risk_score
   FROM folio.waybills;


--
-- Name: waybill_stage_claims; Type: TABLE; Schema: folio; Owner: -
--

CREATE TABLE folio.waybill_stage_claims (
    id bigint NOT NULL,
    waybill_id text NOT NULL,
    stage text NOT NULL,
    claimed_by integer NOT NULL,
    claimed_at timestamp with time zone DEFAULT now() NOT NULL,
    released_at timestamp with time zone,
    released_by integer,
    release_reason text,
    CONSTRAINT waybill_stage_claims_check CHECK ((((released_at IS NULL) AND (released_by IS NULL)) OR (released_at IS NOT NULL))),
    CONSTRAINT waybill_stage_claims_stage_check CHECK ((stage = ANY (ARRAY['accounting_review'::text, 'payment'::text, 'settlement'::text])))
);


--
-- Name: waybill_stage_claims_id_seq; Type: SEQUENCE; Schema: folio; Owner: -
--

CREATE SEQUENCE folio.waybill_stage_claims_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: waybill_stage_claims_id_seq; Type: SEQUENCE OWNED BY; Schema: folio; Owner: -
--

ALTER SEQUENCE folio.waybill_stage_claims_id_seq OWNED BY folio.waybill_stage_claims.id;


--
-- Name: waybill_watchers; Type: TABLE; Schema: folio; Owner: -
--

CREATE TABLE folio.waybill_watchers (
    id bigint NOT NULL,
    waybill_id text NOT NULL,
    stage_key text NOT NULL,
    user_id integer NOT NULL,
    notified_at timestamp without time zone,
    created_at timestamp without time zone DEFAULT now() NOT NULL
);


--
-- Name: waybill_watchers_id_seq; Type: SEQUENCE; Schema: folio; Owner: -
--

CREATE SEQUENCE folio.waybill_watchers_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: waybill_watchers_id_seq; Type: SEQUENCE OWNED BY; Schema: folio; Owner: -
--

ALTER SEQUENCE folio.waybill_watchers_id_seq OWNED BY folio.waybill_watchers.id;


--
-- Name: waybills_fy_2026_seq; Type: SEQUENCE; Schema: folio; Owner: -
--

CREATE SEQUENCE folio.waybills_fy_2026_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: hook_events; Type: TABLE; Schema: hook; Owner: -
--

CREATE TABLE hook.hook_events (
    id bigint NOT NULL,
    provider_id text NOT NULL,
    external_id text,
    event_type text NOT NULL,
    received_at timestamp with time zone DEFAULT now() NOT NULL,
    payload jsonb NOT NULL,
    headers jsonb NOT NULL,
    signature_ok boolean NOT NULL,
    status text DEFAULT 'received'::text NOT NULL,
    processed_at timestamp with time zone,
    processed_by text,
    error text,
    replay_count integer DEFAULT 0 NOT NULL,
    CONSTRAINT hook_events_status_check CHECK ((status = ANY (ARRAY['received'::text, 'processed'::text, 'failed'::text, 'rejected'::text])))
);


--
-- Name: hook_events_id_seq; Type: SEQUENCE; Schema: hook; Owner: -
--

CREATE SEQUENCE hook.hook_events_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: hook_events_id_seq; Type: SEQUENCE OWNED BY; Schema: hook; Owner: -
--

ALTER SEQUENCE hook.hook_events_id_seq OWNED BY hook.hook_events.id;


--
-- Name: hook_providers; Type: TABLE; Schema: hook; Owner: -
--

CREATE TABLE hook.hook_providers (
    id text NOT NULL,
    display_name text NOT NULL,
    kind text NOT NULL,
    secret_env text NOT NULL,
    enabled boolean DEFAULT true NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT hook_providers_kind_check CHECK ((kind = ANY (ARRAY['line'::text, 'generic'::text])))
);


--
-- Name: contract_chunks; Type: TABLE; Schema: law; Owner: -
--

CREATE TABLE law.contract_chunks (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    contract_id uuid NOT NULL,
    chunk_index integer NOT NULL,
    content text NOT NULL,
    token_count integer,
    embedding public.vector(1024),
    metadata jsonb,
    created_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: contract_pages; Type: TABLE; Schema: law; Owner: -
--

CREATE TABLE law.contract_pages (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    contract_id uuid NOT NULL,
    page_index integer NOT NULL,
    image_data bytea NOT NULL,
    image_mime text DEFAULT 'image/jpeg'::text NOT NULL,
    bytes integer,
    created_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: contracts; Type: TABLE; Schema: law; Owner: -
--

CREATE TABLE law.contracts (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    line_user_id text,
    line_group_id text,
    line_message_id text,
    file_name text NOT NULL,
    file_type text,
    file_mime text,
    file_data bytea,
    storage_bucket text,
    storage_path text,
    size_bytes bigint,
    chunk_count integer DEFAULT 0 NOT NULL,
    status text DEFAULT 'pending'::text NOT NULL,
    error_message text,
    uploaded_at timestamp with time zone DEFAULT now() NOT NULL,
    doc_no text,
    category text,
    source text,
    metadata jsonb,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    page_count integer DEFAULT 0 NOT NULL
);


--
-- Name: job_queue; Type: TABLE; Schema: law; Owner: -
--

CREATE TABLE law.job_queue (
    id bigint NOT NULL,
    contract_id uuid NOT NULL,
    raw_text text,
    status text DEFAULT 'pending'::text NOT NULL,
    attempts integer DEFAULT 0 NOT NULL,
    last_error text,
    enqueued_at timestamp with time zone DEFAULT now() NOT NULL,
    started_at timestamp with time zone,
    finished_at timestamp with time zone,
    CONSTRAINT job_queue_status_check CHECK ((status = ANY (ARRAY['pending'::text, 'running'::text, 'done'::text, 'failed'::text])))
);


--
-- Name: job_queue_id_seq; Type: SEQUENCE; Schema: law; Owner: -
--

CREATE SEQUENCE law.job_queue_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: job_queue_id_seq; Type: SEQUENCE OWNED BY; Schema: law; Owner: -
--

ALTER SEQUENCE law.job_queue_id_seq OWNED BY law.job_queue.id;


--
-- Name: user_permissions; Type: TABLE; Schema: perm; Owner: -
--

CREATE TABLE perm.user_permissions (
    id bigint NOT NULL,
    user_id integer NOT NULL,
    permission_id text NOT NULL,
    granted_by text NOT NULL,
    reason text,
    granted_at timestamp with time zone DEFAULT now() NOT NULL,
    revoked_at timestamp with time zone,
    revoked_by text,
    starts_at timestamp with time zone DEFAULT now() NOT NULL,
    ends_at timestamp with time zone,
    CONSTRAINT perm_user_perm_end_after_start CHECK (((ends_at IS NULL) OR (ends_at > starts_at)))
);


--
-- Name: active_user_permissions; Type: VIEW; Schema: perm; Owner: -
--

CREATE VIEW perm.active_user_permissions AS
 SELECT id,
    user_id,
    permission_id,
    granted_by,
    reason,
    granted_at,
    revoked_at,
    revoked_by,
    starts_at,
    ends_at
   FROM perm.user_permissions
  WHERE ((revoked_at IS NULL) AND ((ends_at IS NULL) OR (ends_at > statement_timestamp())));


--
-- Name: audit; Type: TABLE; Schema: perm; Owner: -
--

CREATE TABLE perm.audit (
    id bigint NOT NULL,
    kind text NOT NULL,
    actor text DEFAULT 'system'::text NOT NULL,
    target jsonb NOT NULL,
    occurred_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: audit_id_seq; Type: SEQUENCE; Schema: perm; Owner: -
--

CREATE SEQUENCE perm.audit_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: audit_id_seq; Type: SEQUENCE OWNED BY; Schema: perm; Owner: -
--

ALTER SEQUENCE perm.audit_id_seq OWNED BY perm.audit.id;


--
-- Name: decision_log; Type: TABLE; Schema: perm; Owner: -
--

CREATE TABLE perm.decision_log (
    id bigint NOT NULL,
    actor_user_id integer,
    action_kind text NOT NULL,
    action_target text NOT NULL,
    resource_type text,
    resource_id text,
    decision text NOT NULL,
    reason text,
    matched_perm text,
    matched_policy text,
    occurred_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: decision_log_id_seq; Type: SEQUENCE; Schema: perm; Owner: -
--

CREATE SEQUENCE perm.decision_log_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: decision_log_id_seq; Type: SEQUENCE OWNED BY; Schema: perm; Owner: -
--

ALTER SEQUENCE perm.decision_log_id_seq OWNED BY perm.decision_log.id;


--
-- Name: department_permissions; Type: TABLE; Schema: perm; Owner: -
--

CREATE TABLE perm.department_permissions (
    department_id text NOT NULL,
    permission_id text NOT NULL,
    granted_at timestamp with time zone DEFAULT now() NOT NULL,
    granted_by text DEFAULT 'system'::text NOT NULL,
    significance boolean DEFAULT true NOT NULL
);


--
-- Name: departments; Type: TABLE; Schema: perm; Owner: -
--

CREATE TABLE perm.departments (
    id text NOT NULL,
    display_name text NOT NULL,
    head_user_id integer,
    is_system boolean DEFAULT true NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT departments_id_check CHECK ((id ~ '^[a-z][a-z0-9_-]*$'::text))
);


--
-- Name: permissions; Type: TABLE; Schema: perm; Owner: -
--

CREATE TABLE perm.permissions (
    id text NOT NULL,
    description text
);


--
-- Name: policies; Type: TABLE; Schema: perm; Owner: -
--

CREATE TABLE perm.policies (
    id text NOT NULL,
    name text NOT NULL,
    ast jsonb NOT NULL,
    description text,
    enabled boolean DEFAULT true NOT NULL,
    version integer DEFAULT 1 NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: policy_decisions; Type: TABLE; Schema: perm; Owner: -
--

CREATE TABLE perm.policy_decisions (
    id bigint NOT NULL,
    actor_id integer,
    policy_id text,
    surface text NOT NULL,
    target text,
    decision text NOT NULL,
    reasons jsonb,
    resource jsonb,
    occurred_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: policy_decisions_id_seq; Type: SEQUENCE; Schema: perm; Owner: -
--

CREATE SEQUENCE perm.policy_decisions_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: policy_decisions_id_seq; Type: SEQUENCE OWNED BY; Schema: perm; Owner: -
--

ALTER SEQUENCE perm.policy_decisions_id_seq OWNED BY perm.policy_decisions.id;


--
-- Name: role_permissions; Type: TABLE; Schema: perm; Owner: -
--

CREATE TABLE perm.role_permissions (
    role_id text NOT NULL,
    role_kind text NOT NULL,
    permission_id text NOT NULL,
    granted_at timestamp with time zone DEFAULT now() NOT NULL,
    granted_by text DEFAULT 'system'::text NOT NULL,
    significance boolean DEFAULT false NOT NULL
);


--
-- Name: roles; Type: TABLE; Schema: perm; Owner: -
--

CREATE TABLE perm.roles (
    id text NOT NULL,
    display_name text NOT NULL,
    description text,
    kind text NOT NULL,
    rank smallint,
    is_system boolean DEFAULT true NOT NULL,
    sort_order integer DEFAULT 0 NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    department_id text,
    CONSTRAINT roles_check CHECK ((((kind = 'hierarchy'::text) AND ((rank >= 1) AND (rank <= 7))) OR ((kind = 'system'::text) AND (rank IS NULL)))),
    CONSTRAINT roles_department_kind_check CHECK ((((kind = 'hierarchy'::text) AND (department_id IS NOT NULL)) OR ((kind = 'system'::text) AND (department_id IS NULL)))),
    CONSTRAINT roles_id_check CHECK ((id ~ '^[a-z][a-z0-9_-]*$'::text)),
    CONSTRAINT roles_kind_check CHECK ((kind = ANY (ARRAY['hierarchy'::text, 'system'::text])))
);


--
-- Name: tiles; Type: TABLE; Schema: perm; Owner: -
--

CREATE TABLE perm.tiles (
    id text NOT NULL,
    display_name text NOT NULL,
    subtitle text DEFAULT ''::text NOT NULL,
    icon text DEFAULT '🧾'::text NOT NULL,
    accent text DEFAULT 'slate'::text NOT NULL,
    group_name text NOT NULL,
    sub_view text,
    href text NOT NULL,
    request_target text,
    sort_order integer DEFAULT 0 NOT NULL,
    is_system boolean DEFAULT true NOT NULL,
    owner_group_id text,
    view_perm_id text NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: user_departments; Type: TABLE; Schema: perm; Owner: -
--

CREATE TABLE perm.user_departments (
    user_id integer NOT NULL,
    department_id text NOT NULL,
    assigned_at timestamp with time zone DEFAULT now() NOT NULL,
    assigned_by integer
);


--
-- Name: user_permissions_id_seq; Type: SEQUENCE; Schema: perm; Owner: -
--

CREATE SEQUENCE perm.user_permissions_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: user_permissions_id_seq; Type: SEQUENCE OWNED BY; Schema: perm; Owner: -
--

ALTER SEQUENCE perm.user_permissions_id_seq OWNED BY perm.user_permissions.id;


--
-- Name: user_roles; Type: TABLE; Schema: perm; Owner: -
--

CREATE TABLE perm.user_roles (
    user_id integer NOT NULL,
    role_id text NOT NULL,
    role_kind text NOT NULL,
    granted_at timestamp with time zone DEFAULT now() NOT NULL,
    granted_by text
);


--
-- Name: messages id; Type: DEFAULT; Schema: chat; Owner: -
--

ALTER TABLE ONLY chat.messages ALTER COLUMN id SET DEFAULT nextval('chat.messages_id_seq'::regclass);


--
-- Name: access_requests id; Type: DEFAULT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.access_requests ALTER COLUMN id SET DEFAULT nextval('folio.access_requests_id_seq'::regclass);


--
-- Name: ai_assignments id; Type: DEFAULT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.ai_assignments ALTER COLUMN id SET DEFAULT nextval('folio.ai_assignments_id_seq'::regclass);


--
-- Name: ai_chat_sessions id; Type: DEFAULT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.ai_chat_sessions ALTER COLUMN id SET DEFAULT nextval('folio.ai_chat_sessions_id_seq'::regclass);


--
-- Name: ai_invocations id; Type: DEFAULT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.ai_invocations ALTER COLUMN id SET DEFAULT nextval('folio.ai_invocations_id_seq'::regclass);


--
-- Name: ai_models id; Type: DEFAULT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.ai_models ALTER COLUMN id SET DEFAULT nextval('folio.ai_models_id_seq'::regclass);


--
-- Name: ai_providers id; Type: DEFAULT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.ai_providers ALTER COLUMN id SET DEFAULT nextval('folio.ai_providers_id_seq'::regclass);


--
-- Name: ai_staff id; Type: DEFAULT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.ai_staff ALTER COLUMN id SET DEFAULT nextval('folio.ai_staff_id_seq'::regclass);


--
-- Name: approval_override_audit id; Type: DEFAULT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.approval_override_audit ALTER COLUMN id SET DEFAULT nextval('folio.approval_override_audit_id_seq'::regclass);


--
-- Name: approval_transitions id; Type: DEFAULT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.approval_transitions ALTER COLUMN id SET DEFAULT nextval('folio.approval_transitions_id_seq'::regclass);


--
-- Name: approver_nudges id; Type: DEFAULT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.approver_nudges ALTER COLUMN id SET DEFAULT nextval('folio.approver_nudges_id_seq'::regclass);


--
-- Name: customer_contacts id; Type: DEFAULT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.customer_contacts ALTER COLUMN id SET DEFAULT nextval('folio.customer_contacts_id_seq'::regclass);


--
-- Name: customers id; Type: DEFAULT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.customers ALTER COLUMN id SET DEFAULT nextval('folio.customers_id_seq'::regclass);


--
-- Name: domain_events id; Type: DEFAULT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.domain_events ALTER COLUMN id SET DEFAULT nextval('folio.domain_events_id_seq'::regclass);


--
-- Name: exec_snapshots id; Type: DEFAULT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.exec_snapshots ALTER COLUMN id SET DEFAULT nextval('folio.exec_snapshots_id_seq'::regclass);


--
-- Name: expense_items id; Type: DEFAULT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.expense_items ALTER COLUMN id SET DEFAULT nextval('folio.expense_items_id_seq'::regclass);


--
-- Name: expense_payments id; Type: DEFAULT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.expense_payments ALTER COLUMN id SET DEFAULT nextval('folio.expense_payments_id_seq'::regclass);


--
-- Name: expenses id; Type: DEFAULT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.expenses ALTER COLUMN id SET DEFAULT nextval('folio.expenses_id_seq'::regclass);


--
-- Name: journal_entries id; Type: DEFAULT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.journal_entries ALTER COLUMN id SET DEFAULT nextval('folio.journal_entries_id_seq'::regclass);


--
-- Name: ledger_lines id; Type: DEFAULT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.ledger_lines ALTER COLUMN id SET DEFAULT nextval('folio.ledger_lines_id_seq'::regclass);


--
-- Name: notification_digests id; Type: DEFAULT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.notification_digests ALTER COLUMN id SET DEFAULT nextval('folio.notification_digests_id_seq'::regclass);


--
-- Name: notifications id; Type: DEFAULT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.notifications ALTER COLUMN id SET DEFAULT nextval('folio.notifications_id_seq'::regclass);


--
-- Name: po_invoices id; Type: DEFAULT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.po_invoices ALTER COLUMN id SET DEFAULT nextval('folio.po_invoices_id_seq'::regclass);


--
-- Name: po_items id; Type: DEFAULT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.po_items ALTER COLUMN id SET DEFAULT nextval('folio.po_items_id_seq'::regclass);


--
-- Name: policy_audit id; Type: DEFAULT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.policy_audit ALTER COLUMN id SET DEFAULT nextval('folio.policy_audit_id_seq'::regclass);


--
-- Name: pr_items id; Type: DEFAULT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.pr_items ALTER COLUMN id SET DEFAULT nextval('folio.pr_items_id_seq'::regclass);


--
-- Name: purchase_orders id; Type: DEFAULT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.purchase_orders ALTER COLUMN id SET DEFAULT nextval('folio.purchase_orders_id_seq'::regclass);


--
-- Name: purchase_requisitions id; Type: DEFAULT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.purchase_requisitions ALTER COLUMN id SET DEFAULT nextval('folio.purchase_requisitions_id_seq'::regclass);


--
-- Name: sales_orders id; Type: DEFAULT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.sales_orders ALTER COLUMN id SET DEFAULT nextval('folio.sales_orders_id_seq'::regclass);


--
-- Name: sales_product_embeddings id; Type: DEFAULT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.sales_product_embeddings ALTER COLUMN id SET DEFAULT nextval('folio.sales_product_embeddings_id_seq'::regclass);


--
-- Name: slips id; Type: DEFAULT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.slips ALTER COLUMN id SET DEFAULT nextval('folio.slips_id_seq'::regclass);


--
-- Name: so_items id; Type: DEFAULT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.so_items ALTER COLUMN id SET DEFAULT nextval('folio.so_items_id_seq'::regclass);


--
-- Name: users id; Type: DEFAULT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.users ALTER COLUMN id SET DEFAULT nextval('folio.users_id_seq'::regclass);


--
-- Name: vendor_embeddings id; Type: DEFAULT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.vendor_embeddings ALTER COLUMN id SET DEFAULT nextval('folio.vendor_embeddings_id_seq'::regclass);


--
-- Name: waybill_attachments id; Type: DEFAULT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.waybill_attachments ALTER COLUMN id SET DEFAULT nextval('folio.waybill_attachments_id_seq'::regclass);


--
-- Name: waybill_stage_claims id; Type: DEFAULT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.waybill_stage_claims ALTER COLUMN id SET DEFAULT nextval('folio.waybill_stage_claims_id_seq'::regclass);


--
-- Name: waybill_watchers id; Type: DEFAULT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.waybill_watchers ALTER COLUMN id SET DEFAULT nextval('folio.waybill_watchers_id_seq'::regclass);


--
-- Name: hook_events id; Type: DEFAULT; Schema: hook; Owner: -
--

ALTER TABLE ONLY hook.hook_events ALTER COLUMN id SET DEFAULT nextval('hook.hook_events_id_seq'::regclass);


--
-- Name: job_queue id; Type: DEFAULT; Schema: law; Owner: -
--

ALTER TABLE ONLY law.job_queue ALTER COLUMN id SET DEFAULT nextval('law.job_queue_id_seq'::regclass);


--
-- Name: audit id; Type: DEFAULT; Schema: perm; Owner: -
--

ALTER TABLE ONLY perm.audit ALTER COLUMN id SET DEFAULT nextval('perm.audit_id_seq'::regclass);


--
-- Name: decision_log id; Type: DEFAULT; Schema: perm; Owner: -
--

ALTER TABLE ONLY perm.decision_log ALTER COLUMN id SET DEFAULT nextval('perm.decision_log_id_seq'::regclass);


--
-- Name: policy_decisions id; Type: DEFAULT; Schema: perm; Owner: -
--

ALTER TABLE ONLY perm.policy_decisions ALTER COLUMN id SET DEFAULT nextval('perm.policy_decisions_id_seq'::regclass);


--
-- Name: user_permissions id; Type: DEFAULT; Schema: perm; Owner: -
--

ALTER TABLE ONLY perm.user_permissions ALTER COLUMN id SET DEFAULT nextval('perm.user_permissions_id_seq'::regclass);


--
-- Name: model_ratings model_ratings_pkey; Type: CONSTRAINT; Schema: ai; Owner: -
--

ALTER TABLE ONLY ai.model_ratings
    ADD CONSTRAINT model_ratings_pkey PRIMARY KEY (model_name);


--
-- Name: sessions sessions_pkey; Type: CONSTRAINT; Schema: auth; Owner: -
--

ALTER TABLE ONLY auth.sessions
    ADD CONSTRAINT sessions_pkey PRIMARY KEY (id);


--
-- Name: messages messages_pkey; Type: CONSTRAINT; Schema: chat; Owner: -
--

ALTER TABLE ONLY chat.messages
    ADD CONSTRAINT messages_pkey PRIMARY KEY (id);


--
-- Name: sessions sessions_pkey; Type: CONSTRAINT; Schema: chat; Owner: -
--

ALTER TABLE ONLY chat.sessions
    ADD CONSTRAINT sessions_pkey PRIMARY KEY (id);


--
-- Name: account_cf_class account_cf_class_pkey; Type: CONSTRAINT; Schema: finance; Owner: -
--

ALTER TABLE ONLY finance.account_cf_class
    ADD CONSTRAINT account_cf_class_pkey PRIMARY KEY (account_code);


--
-- Name: ai_provider_health ai_provider_health_pkey; Type: CONSTRAINT; Schema: finance; Owner: -
--

ALTER TABLE ONLY finance.ai_provider_health
    ADD CONSTRAINT ai_provider_health_pkey PRIMARY KEY (provider_id);


--
-- Name: ai_user_section_defaults ai_user_section_defaults_pkey; Type: CONSTRAINT; Schema: finance; Owner: -
--

ALTER TABLE ONLY finance.ai_user_section_defaults
    ADD CONSTRAINT ai_user_section_defaults_pkey PRIMARY KEY (user_id, section_key);


--
-- Name: cashflow_period cashflow_period_pkey; Type: CONSTRAINT; Schema: finance; Owner: -
--

ALTER TABLE ONLY finance.cashflow_period
    ADD CONSTRAINT cashflow_period_pkey PRIMARY KEY (fiscal_year);


--
-- Name: access_requests access_requests_pkey; Type: CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.access_requests
    ADD CONSTRAINT access_requests_pkey PRIMARY KEY (id);


--
-- Name: ai_assignments ai_assignments_pkey; Type: CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.ai_assignments
    ADD CONSTRAINT ai_assignments_pkey PRIMARY KEY (id);


--
-- Name: ai_assignments ai_assignments_unique; Type: CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.ai_assignments
    ADD CONSTRAINT ai_assignments_unique UNIQUE (section_key, task_type, priority);


--
-- Name: ai_chat_sessions ai_chat_sessions_pkey; Type: CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.ai_chat_sessions
    ADD CONSTRAINT ai_chat_sessions_pkey PRIMARY KEY (id);


--
-- Name: ai_chat_sessions ai_chat_sessions_user_id_scope_key; Type: CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.ai_chat_sessions
    ADD CONSTRAINT ai_chat_sessions_user_id_scope_key UNIQUE (user_id, scope);


--
-- Name: ai_invocations ai_invocations_pkey; Type: CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.ai_invocations
    ADD CONSTRAINT ai_invocations_pkey PRIMARY KEY (id);


--
-- Name: ai_models ai_models_pkey; Type: CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.ai_models
    ADD CONSTRAINT ai_models_pkey PRIMARY KEY (id);


--
-- Name: ai_models ai_models_provider_id_name_key; Type: CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.ai_models
    ADD CONSTRAINT ai_models_provider_id_name_key UNIQUE (provider_id, name);


--
-- Name: ai_providers ai_providers_name_key; Type: CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.ai_providers
    ADD CONSTRAINT ai_providers_name_key UNIQUE (name);


--
-- Name: ai_providers ai_providers_pkey; Type: CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.ai_providers
    ADD CONSTRAINT ai_providers_pkey PRIMARY KEY (id);


--
-- Name: ai_staff ai_staff_pkey; Type: CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.ai_staff
    ADD CONSTRAINT ai_staff_pkey PRIMARY KEY (id);


--
-- Name: approval_override_audit approval_override_audit_pkey; Type: CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.approval_override_audit
    ADD CONSTRAINT approval_override_audit_pkey PRIMARY KEY (id);


--
-- Name: approval_transitions approval_transitions_pkey; Type: CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.approval_transitions
    ADD CONSTRAINT approval_transitions_pkey PRIMARY KEY (id);


--
-- Name: approver_nudges approver_nudges_approver_user_id_waybill_id_stage_key; Type: CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.approver_nudges
    ADD CONSTRAINT approver_nudges_approver_user_id_waybill_id_stage_key UNIQUE (approver_user_id, waybill_id, stage);


--
-- Name: approver_nudges approver_nudges_pkey; Type: CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.approver_nudges
    ADD CONSTRAINT approver_nudges_pkey PRIMARY KEY (id);


--
-- Name: chart_of_accounts chart_of_accounts_pkey; Type: CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.chart_of_accounts
    ADD CONSTRAINT chart_of_accounts_pkey PRIMARY KEY (code);


--
-- Name: customer_advisories customer_advisories_pkey; Type: CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.customer_advisories
    ADD CONSTRAINT customer_advisories_pkey PRIMARY KEY (customer_id);


--
-- Name: customer_contacts customer_contacts_pkey; Type: CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.customer_contacts
    ADD CONSTRAINT customer_contacts_pkey PRIMARY KEY (id);


--
-- Name: customers customers_code_key; Type: CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.customers
    ADD CONSTRAINT customers_code_key UNIQUE (code);


--
-- Name: customers customers_pkey; Type: CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.customers
    ADD CONSTRAINT customers_pkey PRIMARY KEY (id);


--
-- Name: domain_events domain_events_pkey; Type: CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.domain_events
    ADD CONSTRAINT domain_events_pkey PRIMARY KEY (id);


--
-- Name: exec_snapshots exec_snapshots_pkey; Type: CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.exec_snapshots
    ADD CONSTRAINT exec_snapshots_pkey PRIMARY KEY (id);


--
-- Name: exec_snapshots exec_snapshots_snapshot_date_key; Type: CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.exec_snapshots
    ADD CONSTRAINT exec_snapshots_snapshot_date_key UNIQUE (snapshot_date);


--
-- Name: expense_items expense_items_pkey; Type: CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.expense_items
    ADD CONSTRAINT expense_items_pkey PRIMARY KEY (id);


--
-- Name: expense_payments expense_payments_expense_id_key; Type: CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.expense_payments
    ADD CONSTRAINT expense_payments_expense_id_key UNIQUE (expense_id);


--
-- Name: expense_payments expense_payments_pkey; Type: CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.expense_payments
    ADD CONSTRAINT expense_payments_pkey PRIMARY KEY (id);


--
-- Name: expense_payments expense_payments_slip_id_key; Type: CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.expense_payments
    ADD CONSTRAINT expense_payments_slip_id_key UNIQUE (slip_id);


--
-- Name: expense_payments expense_payments_waybill_id_key; Type: CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.expense_payments
    ADD CONSTRAINT expense_payments_waybill_id_key UNIQUE (waybill_id);


--
-- Name: expenses expenses_pkey; Type: CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.expenses
    ADD CONSTRAINT expenses_pkey PRIMARY KEY (id);


--
-- Name: hr_leave hr_leave_pkey; Type: CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.hr_leave
    ADD CONSTRAINT hr_leave_pkey PRIMARY KEY (waybill_id);


--
-- Name: journal_entries journal_entries_pkey; Type: CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.journal_entries
    ADD CONSTRAINT journal_entries_pkey PRIMARY KEY (id);


--
-- Name: learned_mappings learned_mappings_pkey; Type: CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.learned_mappings
    ADD CONSTRAINT learned_mappings_pkey PRIMARY KEY (vendor_name_norm, account_code);


--
-- Name: ledger_lines ledger_lines_pkey; Type: CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.ledger_lines
    ADD CONSTRAINT ledger_lines_pkey PRIMARY KEY (id);


--
-- Name: notification_digests notification_digests_pkey; Type: CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.notification_digests
    ADD CONSTRAINT notification_digests_pkey PRIMARY KEY (id);


--
-- Name: notification_digests notification_digests_user_id_period_start_period_end_key; Type: CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.notification_digests
    ADD CONSTRAINT notification_digests_user_id_period_start_period_end_key UNIQUE (user_id, period_start, period_end);


--
-- Name: notifications notifications_pkey; Type: CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.notifications
    ADD CONSTRAINT notifications_pkey PRIMARY KEY (id);

--
-- Name: notifications notifications_event_user_key; Type: CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.notifications
    ADD CONSTRAINT notifications_event_user_key UNIQUE (event_id, user_id, message_key);


--
-- Name: po_invoices po_invoices_pkey; Type: CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.po_invoices
    ADD CONSTRAINT po_invoices_pkey PRIMARY KEY (id);


--
-- Name: po_items po_items_pkey; Type: CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.po_items
    ADD CONSTRAINT po_items_pkey PRIMARY KEY (id);


--
-- Name: policy_audit policy_audit_pkey; Type: CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.policy_audit
    ADD CONSTRAINT policy_audit_pkey PRIMARY KEY (id);


--
-- Name: policy_lint_results policy_lint_results_pkey; Type: CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.policy_lint_results
    ADD CONSTRAINT policy_lint_results_pkey PRIMARY KEY (policy_id);


--
-- Name: pr_items pr_items_pkey; Type: CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.pr_items
    ADD CONSTRAINT pr_items_pkey PRIMARY KEY (id);


--
-- Name: purchase_orders purchase_orders_pkey; Type: CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.purchase_orders
    ADD CONSTRAINT purchase_orders_pkey PRIMARY KEY (id);


--
-- Name: purchase_orders purchase_orders_po_number_key; Type: CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.purchase_orders
    ADD CONSTRAINT purchase_orders_po_number_key UNIQUE (po_number);


--
-- Name: purchase_requisitions purchase_requisitions_pkey; Type: CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.purchase_requisitions
    ADD CONSTRAINT purchase_requisitions_pkey PRIMARY KEY (id);


--
-- Name: sales_orders sales_orders_pkey; Type: CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.sales_orders
    ADD CONSTRAINT sales_orders_pkey PRIMARY KEY (id);


--
-- Name: sales_orders sales_orders_so_number_key; Type: CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.sales_orders
    ADD CONSTRAINT sales_orders_so_number_key UNIQUE (so_number);


--
-- Name: sales_product_embeddings sales_product_embeddings_pkey; Type: CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.sales_product_embeddings
    ADD CONSTRAINT sales_product_embeddings_pkey PRIMARY KEY (id);


--
-- Name: slips slips_pkey; Type: CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.slips
    ADD CONSTRAINT slips_pkey PRIMARY KEY (id);


--
-- Name: so_items so_items_pkey; Type: CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.so_items
    ADD CONSTRAINT so_items_pkey PRIMARY KEY (id);


--
-- Name: users users_employee_code_key; Type: CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.users
    ADD CONSTRAINT users_employee_code_key UNIQUE (employee_code);


--
-- Name: users users_line_user_id_key; Type: CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.users
    ADD CONSTRAINT users_line_user_id_key UNIQUE (line_user_id);


--
-- Name: users users_pkey; Type: CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.users
    ADD CONSTRAINT users_pkey PRIMARY KEY (id);


--
-- Name: vendor_embeddings vendor_embeddings_pkey; Type: CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.vendor_embeddings
    ADD CONSTRAINT vendor_embeddings_pkey PRIMARY KEY (id);


--
-- Name: vision_chain vision_chain_pkey; Type: CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.vision_chain
    ADD CONSTRAINT vision_chain_pkey PRIMARY KEY (section_key);


--
-- Name: waybill_attachments waybill_attachments_pkey; Type: CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.waybill_attachments
    ADD CONSTRAINT waybill_attachments_pkey PRIMARY KEY (id);


--
-- Name: waybill_events waybill_events_pkey; Type: CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.waybill_events
    ADD CONSTRAINT waybill_events_pkey PRIMARY KEY (id);


--
-- Name: waybill_reviews waybill_reviews_pkey; Type: CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.waybill_reviews
    ADD CONSTRAINT waybill_reviews_pkey PRIMARY KEY (waybill_id, stage);


--
-- Name: waybill_stage_claims waybill_stage_claims_pkey; Type: CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.waybill_stage_claims
    ADD CONSTRAINT waybill_stage_claims_pkey PRIMARY KEY (id);


--
-- Name: waybill_watchers waybill_watchers_pkey; Type: CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.waybill_watchers
    ADD CONSTRAINT waybill_watchers_pkey PRIMARY KEY (id);


--
-- Name: waybill_watchers waybill_watchers_waybill_id_stage_key_user_id_key; Type: CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.waybill_watchers
    ADD CONSTRAINT waybill_watchers_waybill_id_stage_key_user_id_key UNIQUE (waybill_id, stage_key, user_id);


--
-- Name: waybills waybills_origin_origin_id_key; Type: CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.waybills
    ADD CONSTRAINT waybills_origin_origin_id_key UNIQUE (origin, origin_id);


--
-- Name: waybills waybills_pkey; Type: CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.waybills
    ADD CONSTRAINT waybills_pkey PRIMARY KEY (id);


--
-- Name: hook_events hook_events_pkey; Type: CONSTRAINT; Schema: hook; Owner: -
--

ALTER TABLE ONLY hook.hook_events
    ADD CONSTRAINT hook_events_pkey PRIMARY KEY (id);


--
-- Name: hook_events hook_events_provider_id_external_id_key; Type: CONSTRAINT; Schema: hook; Owner: -
--

ALTER TABLE ONLY hook.hook_events
    ADD CONSTRAINT hook_events_provider_id_external_id_key UNIQUE (provider_id, external_id);


--
-- Name: hook_providers hook_providers_pkey; Type: CONSTRAINT; Schema: hook; Owner: -
--

ALTER TABLE ONLY hook.hook_providers
    ADD CONSTRAINT hook_providers_pkey PRIMARY KEY (id);


--
-- Name: contract_chunks contract_chunks_contract_id_chunk_index_key; Type: CONSTRAINT; Schema: law; Owner: -
--

ALTER TABLE ONLY law.contract_chunks
    ADD CONSTRAINT contract_chunks_contract_id_chunk_index_key UNIQUE (contract_id, chunk_index);


--
-- Name: contract_chunks contract_chunks_pkey; Type: CONSTRAINT; Schema: law; Owner: -
--

ALTER TABLE ONLY law.contract_chunks
    ADD CONSTRAINT contract_chunks_pkey PRIMARY KEY (id);


--
-- Name: contract_pages contract_pages_contract_id_page_index_key; Type: CONSTRAINT; Schema: law; Owner: -
--

ALTER TABLE ONLY law.contract_pages
    ADD CONSTRAINT contract_pages_contract_id_page_index_key UNIQUE (contract_id, page_index);


--
-- Name: contract_pages contract_pages_pkey; Type: CONSTRAINT; Schema: law; Owner: -
--

ALTER TABLE ONLY law.contract_pages
    ADD CONSTRAINT contract_pages_pkey PRIMARY KEY (id);


--
-- Name: contracts contracts_pkey; Type: CONSTRAINT; Schema: law; Owner: -
--

ALTER TABLE ONLY law.contracts
    ADD CONSTRAINT contracts_pkey PRIMARY KEY (id);


--
-- Name: job_queue job_queue_pkey; Type: CONSTRAINT; Schema: law; Owner: -
--

ALTER TABLE ONLY law.job_queue
    ADD CONSTRAINT job_queue_pkey PRIMARY KEY (id);


--
-- Name: audit audit_pkey; Type: CONSTRAINT; Schema: perm; Owner: -
--

ALTER TABLE ONLY perm.audit
    ADD CONSTRAINT audit_pkey PRIMARY KEY (id);


--
-- Name: decision_log decision_log_pkey; Type: CONSTRAINT; Schema: perm; Owner: -
--

ALTER TABLE ONLY perm.decision_log
    ADD CONSTRAINT decision_log_pkey PRIMARY KEY (id);


--
-- Name: department_permissions department_permissions_pkey; Type: CONSTRAINT; Schema: perm; Owner: -
--

ALTER TABLE ONLY perm.department_permissions
    ADD CONSTRAINT department_permissions_pkey PRIMARY KEY (department_id, permission_id);


--
-- Name: departments departments_display_name_key; Type: CONSTRAINT; Schema: perm; Owner: -
--

ALTER TABLE ONLY perm.departments
    ADD CONSTRAINT departments_display_name_key UNIQUE (display_name);


--
-- Name: departments departments_pkey; Type: CONSTRAINT; Schema: perm; Owner: -
--

ALTER TABLE ONLY perm.departments
    ADD CONSTRAINT departments_pkey PRIMARY KEY (id);


--
-- Name: user_permissions perm_user_permissions_one_alive; Type: CONSTRAINT; Schema: perm; Owner: -
--

ALTER TABLE ONLY perm.user_permissions
    ADD CONSTRAINT perm_user_permissions_one_alive EXCLUDE USING btree (user_id WITH =, permission_id WITH =) WHERE ((revoked_at IS NULL)) DEFERRABLE;


--
-- Name: permissions permissions_pkey; Type: CONSTRAINT; Schema: perm; Owner: -
--

ALTER TABLE ONLY perm.permissions
    ADD CONSTRAINT permissions_pkey PRIMARY KEY (id);


--
-- Name: policies policies_name_key; Type: CONSTRAINT; Schema: perm; Owner: -
--

ALTER TABLE ONLY perm.policies
    ADD CONSTRAINT policies_name_key UNIQUE (name);


--
-- Name: policies policies_pkey; Type: CONSTRAINT; Schema: perm; Owner: -
--

ALTER TABLE ONLY perm.policies
    ADD CONSTRAINT policies_pkey PRIMARY KEY (id);


--
-- Name: policy_decisions policy_decisions_pkey; Type: CONSTRAINT; Schema: perm; Owner: -
--

ALTER TABLE ONLY perm.policy_decisions
    ADD CONSTRAINT policy_decisions_pkey PRIMARY KEY (id);


--
-- Name: role_permissions role_permissions_pkey; Type: CONSTRAINT; Schema: perm; Owner: -
--

ALTER TABLE ONLY perm.role_permissions
    ADD CONSTRAINT role_permissions_pkey PRIMARY KEY (role_id, permission_id);


--
-- Name: roles roles_display_name_key; Type: CONSTRAINT; Schema: perm; Owner: -
--

ALTER TABLE ONLY perm.roles
    ADD CONSTRAINT roles_display_name_key UNIQUE (display_name);


--
-- Name: roles roles_id_kind_key; Type: CONSTRAINT; Schema: perm; Owner: -
--

ALTER TABLE ONLY perm.roles
    ADD CONSTRAINT roles_id_kind_key UNIQUE (id, kind);


--
-- Name: roles roles_pkey; Type: CONSTRAINT; Schema: perm; Owner: -
--

ALTER TABLE ONLY perm.roles
    ADD CONSTRAINT roles_pkey PRIMARY KEY (id);


--
-- Name: tiles tiles_pkey; Type: CONSTRAINT; Schema: perm; Owner: -
--

ALTER TABLE ONLY perm.tiles
    ADD CONSTRAINT tiles_pkey PRIMARY KEY (id);


--
-- Name: user_departments user_departments_pkey; Type: CONSTRAINT; Schema: perm; Owner: -
--

ALTER TABLE ONLY perm.user_departments
    ADD CONSTRAINT user_departments_pkey PRIMARY KEY (user_id);


--
-- Name: user_permissions user_permissions_pkey; Type: CONSTRAINT; Schema: perm; Owner: -
--

ALTER TABLE ONLY perm.user_permissions
    ADD CONSTRAINT user_permissions_pkey PRIMARY KEY (id);


--
-- Name: user_roles user_roles_pkey; Type: CONSTRAINT; Schema: perm; Owner: -
--

ALTER TABLE ONLY perm.user_roles
    ADD CONSTRAINT user_roles_pkey PRIMARY KEY (user_id, role_kind);


--
-- Name: auth_sessions_active_idx; Type: INDEX; Schema: auth; Owner: -
--

CREATE INDEX auth_sessions_active_idx ON auth.sessions USING btree (expires_at) WHERE (revoked_at IS NULL);


--
-- Name: auth_sessions_user_id_idx; Type: INDEX; Schema: auth; Owner: -
--

CREATE INDEX auth_sessions_user_id_idx ON auth.sessions USING btree (user_id);


--
-- Name: chat_messages_session_idx; Type: INDEX; Schema: chat; Owner: -
--

CREATE INDEX chat_messages_session_idx ON chat.messages USING btree (session_id, id);


--
-- Name: chat_sessions_scope_idx; Type: INDEX; Schema: chat; Owner: -
--

CREATE INDEX chat_sessions_scope_idx ON chat.sessions USING btree (user_id, scope, updated_at DESC);


--
-- Name: chat_sessions_user_idx; Type: INDEX; Schema: chat; Owner: -
--

CREATE INDEX chat_sessions_user_idx ON chat.sessions USING btree (user_id, updated_at DESC);


--
-- Name: messages_reasoning_idx; Type: INDEX; Schema: chat; Owner: -
--

CREATE INDEX messages_reasoning_idx ON chat.messages USING btree (session_id) WHERE (reasoning IS NOT NULL);


--
-- Name: account_cf_class_activity_idx; Type: INDEX; Schema: finance; Owner: -
--

CREATE INDEX account_cf_class_activity_idx ON finance.account_cf_class USING btree (activity, is_cash_account);


--
-- Name: idx_ai_user_section_defaults_section; Type: INDEX; Schema: finance; Owner: -
--

CREATE INDEX idx_ai_user_section_defaults_section ON finance.ai_user_section_defaults USING btree (section_key);


--
-- Name: access_requests_pending_uniq; Type: INDEX; Schema: folio; Owner: -
--

CREATE UNIQUE INDEX access_requests_pending_uniq ON folio.access_requests USING btree (actor_id, tile_id) WHERE (status = 'pending'::text);


--
-- Name: access_requests_status_idx; Type: INDEX; Schema: folio; Owner: -
--

CREATE INDEX access_requests_status_idx ON folio.access_requests USING btree (status, created_at DESC);


--
-- Name: access_requests_target_idx; Type: INDEX; Schema: folio; Owner: -
--

CREATE INDEX access_requests_target_idx ON folio.access_requests USING btree (target_user_id) WHERE (status = 'pending'::text);


--
-- Name: customer_contacts_customer_idx; Type: INDEX; Schema: folio; Owner: -
--

CREATE INDEX customer_contacts_customer_idx ON folio.customer_contacts USING btree (customer_id);


--
-- Name: customers_active_idx; Type: INDEX; Schema: folio; Owner: -
--

CREATE INDEX customers_active_idx ON folio.customers USING btree (is_active) WHERE is_active;


--
-- Name: customers_name_idx; Type: INDEX; Schema: folio; Owner: -
--

CREATE INDEX customers_name_idx ON folio.customers USING btree (name);


--
-- Name: exec_snapshots_date_idx; Type: INDEX; Schema: folio; Owner: -
--

CREATE INDEX exec_snapshots_date_idx ON folio.exec_snapshots USING btree (snapshot_date DESC);


--
-- Name: folio_ai_chat_sessions_scope_idx; Type: INDEX; Schema: folio; Owner: -
--

CREATE INDEX folio_ai_chat_sessions_scope_idx ON folio.ai_chat_sessions USING btree (user_id, scope) WHERE (scope ~~ 'hr:%'::text);


--
-- Name: folio_ai_chat_sessions_user_idx; Type: INDEX; Schema: folio; Owner: -
--

CREATE INDEX folio_ai_chat_sessions_user_idx ON folio.ai_chat_sessions USING btree (user_id);


--
-- Name: folio_approver_nudges_approver_idx; Type: INDEX; Schema: folio; Owner: -
--

CREATE INDEX folio_approver_nudges_approver_idx ON folio.approver_nudges USING btree (approver_user_id, sent_at DESC);


--
-- Name: folio_customer_advisories_severity_idx; Type: INDEX; Schema: folio; Owner: -
--

CREATE INDEX folio_customer_advisories_severity_idx ON folio.customer_advisories USING btree (severity);


--
-- Name: folio_customers_embedding_idx; Type: INDEX; Schema: folio; Owner: -
--

CREATE INDEX folio_customers_embedding_idx ON folio.customers USING ivfflat (embedding public.vector_cosine_ops) WITH (lists='100') WHERE (embedding IS NOT NULL);


--
-- Name: folio_learned_mappings_vendor_norm_idx; Type: INDEX; Schema: folio; Owner: -
--

CREATE INDEX folio_learned_mappings_vendor_norm_idx ON folio.learned_mappings USING btree (vendor_name_norm);


--
-- Name: folio_notification_digests_user_idx; Type: INDEX; Schema: folio; Owner: -
--

CREATE INDEX folio_notification_digests_user_idx ON folio.notification_digests USING btree (user_id, generated_at DESC);


--
-- Name: folio_po_invoices_status_idx; Type: INDEX; Schema: folio; Owner: -
--

CREATE INDEX folio_po_invoices_status_idx ON folio.po_invoices USING btree (status, created_at DESC);


--
-- Name: folio_po_invoices_vendor_idx; Type: INDEX; Schema: folio; Owner: -
--

CREATE INDEX folio_po_invoices_vendor_idx ON folio.po_invoices USING btree (vendor_id);


--
-- Name: folio_policy_lint_results_generated_at_idx; Type: INDEX; Schema: folio; Owner: -
--

CREATE INDEX folio_policy_lint_results_generated_at_idx ON folio.policy_lint_results USING btree (generated_at DESC);


--
-- Name: folio_sales_product_embeddings_so_item_idx; Type: INDEX; Schema: folio; Owner: -
--

CREATE INDEX folio_sales_product_embeddings_so_item_idx ON folio.sales_product_embeddings USING btree (so_item_id);


--
-- Name: folio_sales_product_embeddings_vec_idx; Type: INDEX; Schema: folio; Owner: -
--

CREATE INDEX folio_sales_product_embeddings_vec_idx ON folio.sales_product_embeddings USING ivfflat (embedding public.vector_cosine_ops) WITH (lists='100');


--
-- Name: folio_vendor_embeddings_date_idx; Type: INDEX; Schema: folio; Owner: -
--

CREATE INDEX folio_vendor_embeddings_date_idx ON folio.vendor_embeddings USING btree (transaction_date);


--
-- Name: folio_vendor_embeddings_submitter_idx; Type: INDEX; Schema: folio; Owner: -
--

CREATE INDEX folio_vendor_embeddings_submitter_idx ON folio.vendor_embeddings USING btree (submitter_id);


--
-- Name: folio_vendor_embeddings_vec_idx; Type: INDEX; Schema: folio; Owner: -
--

CREATE INDEX folio_vendor_embeddings_vec_idx ON folio.vendor_embeddings USING ivfflat (embedding public.vector_cosine_ops) WITH (lists='100');


--
-- Name: folio_waybills_flagged_reason_idx; Type: INDEX; Schema: folio; Owner: -
--

CREATE INDEX folio_waybills_flagged_reason_idx ON folio.waybills USING gin (flagged_reason) WHERE (flagged_reason IS NOT NULL);


--
-- Name: idx_ai_assignments_section; Type: INDEX; Schema: folio; Owner: -
--

CREATE INDEX idx_ai_assignments_section ON folio.ai_assignments USING btree (section_key, task_type, priority);


--
-- Name: idx_ai_invocations_created; Type: INDEX; Schema: folio; Owner: -
--

CREATE INDEX idx_ai_invocations_created ON folio.ai_invocations USING btree (created_at DESC);


--
-- Name: idx_ai_invocations_section; Type: INDEX; Schema: folio; Owner: -
--

CREATE INDEX idx_ai_invocations_section ON folio.ai_invocations USING btree (section_key, created_at DESC);


--
-- Name: idx_ai_invocations_staff; Type: INDEX; Schema: folio; Owner: -
--

CREATE INDEX idx_ai_invocations_staff ON folio.ai_invocations USING btree (staff_id, created_at DESC);


--
-- Name: idx_ai_models_provider; Type: INDEX; Schema: folio; Owner: -
--

CREATE INDEX idx_ai_models_provider ON folio.ai_models USING btree (provider_id);


--
-- Name: idx_aov_actor; Type: INDEX; Schema: folio; Owner: -
--

CREATE INDEX idx_aov_actor ON folio.approval_override_audit USING btree (actor_id);


--
-- Name: idx_aov_kind_stage; Type: INDEX; Schema: folio; Owner: -
--

CREATE INDEX idx_aov_kind_stage ON folio.approval_override_audit USING btree (kind, attempted_stage);


--
-- Name: idx_aov_target; Type: INDEX; Schema: folio; Owner: -
--

CREATE INDEX idx_aov_target ON folio.approval_override_audit USING btree (target_type, target_id);


--
-- Name: idx_atx_actor; Type: INDEX; Schema: folio; Owner: -
--

CREATE INDEX idx_atx_actor ON folio.approval_transitions USING btree (actor_id);


--
-- Name: idx_atx_stage; Type: INDEX; Schema: folio; Owner: -
--

CREATE INDEX idx_atx_stage ON folio.approval_transitions USING btree (stage) WHERE (stage IS NOT NULL);


--
-- Name: idx_atx_target; Type: INDEX; Schema: folio; Owner: -
--

CREATE INDEX idx_atx_target ON folio.approval_transitions USING btree (target_type, target_id, created_at DESC);


--
-- Name: idx_domain_events_actor; Type: INDEX; Schema: folio; Owner: -
--

CREATE INDEX idx_domain_events_actor ON folio.domain_events USING btree (actor_id);


--
-- Name: idx_domain_events_created_at; Type: INDEX; Schema: folio; Owner: -
--

CREATE INDEX idx_domain_events_created_at ON folio.domain_events USING btree (created_at DESC);


--
-- Name: idx_domain_events_type; Type: INDEX; Schema: folio; Owner: -
--

CREATE INDEX idx_domain_events_type ON folio.domain_events USING btree (type);


--
-- Name: idx_expenses_jv; Type: INDEX; Schema: folio; Owner: -
--

CREATE INDEX idx_expenses_jv ON folio.expenses USING btree (journal_entry_id);


--
-- Name: idx_expenses_po; Type: INDEX; Schema: folio; Owner: -
--

CREATE INDEX idx_expenses_po ON folio.expenses USING btree (po_id);


--
-- Name: idx_expenses_pr; Type: INDEX; Schema: folio; Owner: -
--

CREATE INDEX idx_expenses_pr ON folio.expenses USING btree (pr_id);


--
-- Name: idx_expenses_rejected; Type: INDEX; Schema: folio; Owner: -
--

CREATE INDEX idx_expenses_rejected ON folio.expenses USING btree (rejected_at) WHERE ((status)::text = 'rejected'::text);


--
-- Name: idx_hr_leave_dates; Type: INDEX; Schema: folio; Owner: -
--

CREATE INDEX idx_hr_leave_dates ON folio.hr_leave USING btree (start_date, end_date);


--
-- Name: idx_hr_leave_employee; Type: INDEX; Schema: folio; Owner: -
--

CREATE INDEX idx_hr_leave_employee ON folio.hr_leave USING btree (employee_id);


--
-- Name: idx_journal_entries_active; Type: INDEX; Schema: folio; Owner: -
--

CREATE INDEX idx_journal_entries_active ON folio.journal_entries USING btree (expense_id) WHERE (is_draft = false);


--
-- Name: idx_journal_entries_expense_id; Type: INDEX; Schema: folio; Owner: -
--

CREATE INDEX idx_journal_entries_expense_id ON folio.journal_entries USING btree (expense_id);


--
-- Name: idx_journal_entries_one_draft_per_expense; Type: INDEX; Schema: folio; Owner: -
--

CREATE UNIQUE INDEX idx_journal_entries_one_draft_per_expense ON folio.journal_entries USING btree (expense_id, step) WHERE ((is_draft = true) AND (expense_id IS NOT NULL));


--
-- Name: idx_journal_entries_one_draft_per_po; Type: INDEX; Schema: folio; Owner: -
--

CREATE UNIQUE INDEX idx_journal_entries_one_draft_per_po ON folio.journal_entries USING btree (po_id, step) WHERE ((is_draft = true) AND (po_id IS NOT NULL));


--
-- Name: idx_journal_entries_one_draft_per_pr; Type: INDEX; Schema: folio; Owner: -
--

CREATE UNIQUE INDEX idx_journal_entries_one_draft_per_pr ON folio.journal_entries USING btree (pr_id, step) WHERE ((is_draft = true) AND (pr_id IS NOT NULL));


--
-- Name: idx_journal_entries_one_draft_per_so; Type: INDEX; Schema: folio; Owner: -
--

CREATE INDEX idx_journal_entries_one_draft_per_so ON folio.journal_entries USING btree (so_id, step) WHERE ((is_draft = true) AND (so_id IS NOT NULL));


--
-- Name: idx_journal_entries_po; Type: INDEX; Schema: folio; Owner: -
--

CREATE INDEX idx_journal_entries_po ON folio.journal_entries USING btree (po_id);


--
-- Name: idx_journal_entries_po_step; Type: INDEX; Schema: folio; Owner: -
--

CREATE INDEX idx_journal_entries_po_step ON folio.journal_entries USING btree (po_id, step);


--
-- Name: idx_journal_entries_pr; Type: INDEX; Schema: folio; Owner: -
--

CREATE INDEX idx_journal_entries_pr ON folio.journal_entries USING btree (pr_id);


--
-- Name: idx_journal_entries_pr_step; Type: INDEX; Schema: folio; Owner: -
--

CREATE INDEX idx_journal_entries_pr_step ON folio.journal_entries USING btree (pr_id, step);


--
-- Name: idx_journal_entries_so_id; Type: INDEX; Schema: folio; Owner: -
--

CREATE INDEX idx_journal_entries_so_id ON folio.journal_entries USING btree (so_id);


--
-- Name: idx_ledger_lines_account_code; Type: INDEX; Schema: folio; Owner: -
--

CREATE INDEX idx_ledger_lines_account_code ON folio.ledger_lines USING btree (account_code);


--
-- Name: idx_ledger_lines_journal_entry; Type: INDEX; Schema: folio; Owner: -
--

CREATE INDEX idx_ledger_lines_journal_entry ON folio.ledger_lines USING btree (journal_entry_id);


--
-- Name: idx_notif_user_cleared; Type: INDEX; Schema: folio; Owner: -
--

CREATE INDEX idx_notif_user_unread ON folio.notifications USING btree (user_id, read_at, created_at DESC);


--
-- Name: idx_notif_user_feed; Type: INDEX; Schema: folio; Owner: -
--

CREATE INDEX idx_notif_user_feed ON folio.notifications USING btree (user_id, category, resolved_at, created_at DESC);


--
-- Name: idx_notif_waybill_stage; Type: INDEX; Schema: folio; Owner: -
--

CREATE INDEX idx_notif_waybill_stage ON folio.notifications USING btree (waybill_id, stage_key, category, resolved_at);


--
-- Name: idx_po_items_po; Type: INDEX; Schema: folio; Owner: -
--

CREATE INDEX idx_po_items_po ON folio.po_items USING btree (po_id);


--
-- Name: idx_po_pr; Type: INDEX; Schema: folio; Owner: -
--

CREATE INDEX idx_po_pr ON folio.purchase_orders USING btree (pr_id);


--
-- Name: idx_po_status; Type: INDEX; Schema: folio; Owner: -
--

CREATE INDEX idx_po_status ON folio.purchase_orders USING btree (status);


--
-- Name: idx_pr_dept_group; Type: INDEX; Schema: folio; Owner: -
--

CREATE INDEX idx_pr_dept_group ON folio.purchase_requisitions USING btree (dept_group_id);


--
-- Name: idx_pr_pr_number; Type: INDEX; Schema: folio; Owner: -
--

CREATE UNIQUE INDEX idx_pr_pr_number ON folio.purchase_requisitions USING btree (pr_number);


--
-- Name: idx_pr_status; Type: INDEX; Schema: folio; Owner: -
--

CREATE INDEX idx_pr_status ON folio.purchase_requisitions USING btree (status);


--
-- Name: idx_prs_rejected; Type: INDEX; Schema: folio; Owner: -
--

CREATE INDEX idx_prs_rejected ON folio.purchase_requisitions USING btree (rejected_at) WHERE ((status)::text = 'rejected'::text);


--
-- Name: idx_slips_expense; Type: INDEX; Schema: folio; Owner: -
--

CREATE INDEX idx_slips_expense ON folio.slips USING btree (expense_id);


--
-- Name: idx_slips_kind_expense; Type: INDEX; Schema: folio; Owner: -
--

CREATE INDEX idx_slips_kind_expense ON folio.slips USING btree (kind, expense_id) WHERE (expense_id IS NOT NULL);


--
-- Name: idx_slips_po; Type: INDEX; Schema: folio; Owner: -
--

CREATE INDEX idx_slips_po ON folio.slips USING btree (po_id);


--
-- Name: idx_slips_pr; Type: INDEX; Schema: folio; Owner: -
--

CREATE INDEX idx_slips_pr ON folio.slips USING btree (pr_id);


--
-- Name: idx_slips_status; Type: INDEX; Schema: folio; Owner: -
--

CREATE INDEX idx_slips_status ON folio.slips USING btree (status);


--
-- Name: idx_slips_uploader_status; Type: INDEX; Schema: folio; Owner: -
--

CREATE INDEX idx_slips_uploader_status ON folio.slips USING btree (uploaded_by, status);


--
-- Name: idx_users_active; Type: INDEX; Schema: folio; Owner: -
--

CREATE INDEX idx_users_active ON folio.users USING btree (is_active) WHERE is_active;


--
-- Name: idx_users_dept_label; Type: INDEX; Schema: folio; Owner: -
--

CREATE INDEX idx_users_dept_label ON folio.users USING btree (dept_label) WHERE (dept_label IS NOT NULL);


--
-- Name: idx_users_hired_at; Type: INDEX; Schema: folio; Owner: -
--

CREATE INDEX idx_users_hired_at ON folio.users USING btree (hired_at);


--
-- Name: idx_watchers_user_unread; Type: INDEX; Schema: folio; Owner: -
--

CREATE INDEX idx_watchers_user_unread ON folio.waybill_watchers USING btree (user_id) WHERE (notified_at IS NULL);


--
-- Name: idx_watchers_waybill; Type: INDEX; Schema: folio; Owner: -
--

CREATE INDEX idx_watchers_waybill ON folio.waybill_watchers USING btree (waybill_id);


--
-- Name: idx_waybill_attachments_wb_created; Type: INDEX; Schema: folio; Owner: -
--

CREATE INDEX idx_waybill_attachments_wb_created ON folio.waybill_attachments USING btree (waybill_id, occurred_at DESC);


--
-- Name: idx_waybill_attachments_wb_kind; Type: INDEX; Schema: folio; Owner: -
--

CREATE INDEX idx_waybill_attachments_wb_kind ON folio.waybill_attachments USING btree (waybill_id, kind);


--
-- Name: idx_waybill_attachments_wb_stage; Type: INDEX; Schema: folio; Owner: -
--

CREATE INDEX idx_waybill_attachments_wb_stage ON folio.waybill_attachments USING btree (waybill_id, stage_key);


--
-- Name: idx_waybills_fy; Type: INDEX; Schema: folio; Owner: -
--

CREATE INDEX idx_waybills_fy ON folio.waybills USING btree (fiscal_year);


--
-- Name: idx_waybills_origin; Type: INDEX; Schema: folio; Owner: -
--

CREATE INDEX idx_waybills_origin ON folio.waybills USING btree (origin, origin_id);


--
-- Name: idx_waybills_owner; Type: INDEX; Schema: folio; Owner: -
--

CREATE INDEX idx_waybills_owner ON folio.waybills USING btree (current_owner_user_id);


--
-- Name: idx_waybills_stage; Type: INDEX; Schema: folio; Owner: -
--

CREATE INDEX idx_waybills_stage ON folio.waybills USING btree (current_stage);


--
-- Name: idx_waybills_submitter; Type: INDEX; Schema: folio; Owner: -
--

CREATE INDEX idx_waybills_submitter ON folio.waybills USING btree (submitter_id);


--
-- Name: idx_wbx_actor; Type: INDEX; Schema: folio; Owner: -
--

CREATE INDEX idx_wbx_actor ON folio.waybill_events USING btree (actor_id);


--
-- Name: idx_wbx_kind; Type: INDEX; Schema: folio; Owner: -
--

CREATE INDEX idx_wbx_kind ON folio.waybill_events USING btree (kind);


--
-- Name: idx_wbx_previous; Type: INDEX; Schema: folio; Owner: -
--

CREATE INDEX idx_wbx_previous ON folio.waybill_events USING btree (previous_event_id);


--
-- Name: idx_wbx_waybill; Type: INDEX; Schema: folio; Owner: -
--

CREATE INDEX idx_wbx_waybill ON folio.waybill_events USING btree (waybill_id, sequence);


--
-- Name: sales_orders_customer_idx; Type: INDEX; Schema: folio; Owner: -
--

CREATE INDEX sales_orders_customer_idx ON folio.sales_orders USING btree (customer_id);


--
-- Name: sales_orders_due_idx; Type: INDEX; Schema: folio; Owner: -
--

CREATE INDEX sales_orders_due_idx ON folio.sales_orders USING btree (due_date) WHERE (status <> ALL (ARRAY['so_paid'::text, 'rejected'::text]));


--
-- Name: sales_orders_rep_idx; Type: INDEX; Schema: folio; Owner: -
--

CREATE INDEX sales_orders_rep_idx ON folio.sales_orders USING btree (sales_rep_id);


--
-- Name: sales_orders_status_idx; Type: INDEX; Schema: folio; Owner: -
--

CREATE INDEX sales_orders_status_idx ON folio.sales_orders USING btree (status);


--
-- Name: so_items_so_idx; Type: INDEX; Schema: folio; Owner: -
--

CREATE INDEX so_items_so_idx ON folio.so_items USING btree (sales_order_id);


--
-- Name: waybill_reviews_generated_at_idx; Type: INDEX; Schema: folio; Owner: -
--

CREATE INDEX waybill_reviews_generated_at_idx ON folio.waybill_reviews USING btree (generated_at DESC);


--
-- Name: waybill_stage_claims_actor_idx; Type: INDEX; Schema: folio; Owner: -
--

CREATE INDEX waybill_stage_claims_actor_idx ON folio.waybill_stage_claims USING btree (claimed_by) WHERE (released_at IS NULL);


--
-- Name: waybill_stage_claims_one_open; Type: INDEX; Schema: folio; Owner: -
--

CREATE UNIQUE INDEX waybill_stage_claims_one_open ON folio.waybill_stage_claims USING btree (waybill_id, stage) WHERE (released_at IS NULL);


--
-- Name: hook_events_provider_idx; Type: INDEX; Schema: hook; Owner: -
--

CREATE INDEX hook_events_provider_idx ON hook.hook_events USING btree (provider_id);


--
-- Name: hook_events_received_idx; Type: INDEX; Schema: hook; Owner: -
--

CREATE INDEX hook_events_received_idx ON hook.hook_events USING btree (received_at DESC);


--
-- Name: hook_events_status_idx; Type: INDEX; Schema: hook; Owner: -
--

CREATE INDEX hook_events_status_idx ON hook.hook_events USING btree (status);


--
-- Name: idx_law_chunks_contract; Type: INDEX; Schema: law; Owner: -
--

CREATE INDEX idx_law_chunks_contract ON law.contract_chunks USING btree (contract_id);


--
-- Name: idx_law_chunks_embedding; Type: INDEX; Schema: law; Owner: -
--

CREATE INDEX idx_law_chunks_embedding ON law.contract_chunks USING hnsw (embedding public.vector_cosine_ops);


--
-- Name: idx_law_contracts_doc_no; Type: INDEX; Schema: law; Owner: -
--

CREATE UNIQUE INDEX idx_law_contracts_doc_no ON law.contracts USING btree (doc_no) WHERE (doc_no IS NOT NULL);


--
-- Name: idx_law_contracts_line_user; Type: INDEX; Schema: law; Owner: -
--

CREATE INDEX idx_law_contracts_line_user ON law.contracts USING btree (line_user_id);


--
-- Name: idx_law_contracts_status; Type: INDEX; Schema: law; Owner: -
--

CREATE INDEX idx_law_contracts_status ON law.contracts USING btree (status);


--
-- Name: idx_law_pages_contract; Type: INDEX; Schema: law; Owner: -
--

CREATE INDEX idx_law_pages_contract ON law.contract_pages USING btree (contract_id);


--
-- Name: law_job_queue_contract_idx; Type: INDEX; Schema: law; Owner: -
--

CREATE INDEX law_job_queue_contract_idx ON law.job_queue USING btree (contract_id);


--
-- Name: law_job_queue_status_idx; Type: INDEX; Schema: law; Owner: -
--

CREATE INDEX law_job_queue_status_idx ON law.job_queue USING btree (status, enqueued_at) WHERE (status = 'pending'::text);


--
-- Name: perm_decision_log_action_idx; Type: INDEX; Schema: perm; Owner: -
--

CREATE INDEX perm_decision_log_action_idx ON perm.decision_log USING btree (action_kind, occurred_at DESC);


--
-- Name: perm_decision_log_actor_idx; Type: INDEX; Schema: perm; Owner: -
--

CREATE INDEX perm_decision_log_actor_idx ON perm.decision_log USING btree (actor_user_id, occurred_at DESC);


--
-- Name: perm_decision_log_decision_idx; Type: INDEX; Schema: perm; Owner: -
--

CREATE INDEX perm_decision_log_decision_idx ON perm.decision_log USING btree (decision, occurred_at DESC);


--
-- Name: perm_decision_log_resource_idx; Type: INDEX; Schema: perm; Owner: -
--

CREATE INDEX perm_decision_log_resource_idx ON perm.decision_log USING btree (resource_type, resource_id, occurred_at DESC);


--
-- Name: perm_department_permissions_perm_idx; Type: INDEX; Schema: perm; Owner: -
--

CREATE INDEX perm_department_permissions_perm_idx ON perm.department_permissions USING btree (permission_id);


--
-- Name: perm_one_dept_per_user_idx; Type: INDEX; Schema: perm; Owner: -
--

CREATE UNIQUE INDEX perm_one_dept_per_user_idx ON perm.user_permissions USING btree (user_id) WHERE ((permission_id ~~ 'user:dept:%::allow'::text) AND (revoked_at IS NULL));


--
-- Name: perm_perm_domain_idx; Type: INDEX; Schema: perm; Owner: -
--

CREATE INDEX perm_perm_domain_idx ON perm.permissions USING btree (split_part(id, ':'::text, 1));


--
-- Name: perm_roles_department_idx; Type: INDEX; Schema: perm; Owner: -
--

CREATE INDEX perm_roles_department_idx ON perm.roles USING btree (department_id, rank, sort_order);


--
-- Name: perm_tiles_group_idx; Type: INDEX; Schema: perm; Owner: -
--

CREATE INDEX perm_tiles_group_idx ON perm.tiles USING btree (group_name, sort_order);


--
-- Name: perm_up_active_idx; Type: INDEX; Schema: perm; Owner: -
--

CREATE INDEX perm_up_active_idx ON perm.user_permissions USING btree (user_id) WHERE (revoked_at IS NULL);


--
-- Name: perm_up_perm_idx; Type: INDEX; Schema: perm; Owner: -
--

CREATE INDEX perm_up_perm_idx ON perm.user_permissions USING btree (permission_id) WHERE (revoked_at IS NULL);


--
-- Name: perm_user_departments_dept_idx; Type: INDEX; Schema: perm; Owner: -
--

CREATE INDEX perm_user_departments_dept_idx ON perm.user_departments USING btree (department_id);


--
-- Name: perm_user_roles_role_idx; Type: INDEX; Schema: perm; Owner: -
--

CREATE INDEX perm_user_roles_role_idx ON perm.user_roles USING btree (role_id);


--
-- Name: slips slips_exactly_one_parent_trg; Type: TRIGGER; Schema: folio; Owner: -
--

CREATE CONSTRAINT TRIGGER slips_exactly_one_parent_trg AFTER INSERT OR UPDATE ON folio.slips DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION folio.slips_check_exactly_one_parent();


--
-- Name: TRIGGER slips_exactly_one_parent_trg ON slips; Type: COMMENT; Schema: folio; Owner: -
--

COMMENT ON TRIGGER slips_exactly_one_parent_trg ON folio.slips IS 'Enforces exactly-one of (expense_id, pr_id, po_id). DEFERRABLE so orphan-then-link patterns within a single transaction still commit successfully; permanent orphans (slips committed with zero parents, or with multiple parents) are rejected.';


--
-- Name: contracts trg_law_contracts_touch; Type: TRIGGER; Schema: law; Owner: -
--

CREATE TRIGGER trg_law_contracts_touch BEFORE UPDATE ON law.contracts FOR EACH ROW EXECUTE FUNCTION law.touch_updated_at();


--
-- Name: tiles perm_tiles_touch; Type: TRIGGER; Schema: perm; Owner: -
--

CREATE TRIGGER perm_tiles_touch BEFORE UPDATE ON perm.tiles FOR EACH ROW EXECUTE FUNCTION perm.touch_updated_at();


--
-- Name: sessions sessions_impersonator_user_id_fkey; Type: FK CONSTRAINT; Schema: auth; Owner: -
--

ALTER TABLE ONLY auth.sessions
    ADD CONSTRAINT sessions_impersonator_user_id_fkey FOREIGN KEY (impersonator_user_id) REFERENCES folio.users(id) ON DELETE SET NULL;


--
-- Name: sessions sessions_user_id_fkey; Type: FK CONSTRAINT; Schema: auth; Owner: -
--

ALTER TABLE ONLY auth.sessions
    ADD CONSTRAINT sessions_user_id_fkey FOREIGN KEY (user_id) REFERENCES folio.users(id) ON DELETE CASCADE;


--
-- Name: messages messages_session_id_fkey; Type: FK CONSTRAINT; Schema: chat; Owner: -
--

ALTER TABLE ONLY chat.messages
    ADD CONSTRAINT messages_session_id_fkey FOREIGN KEY (session_id) REFERENCES chat.sessions(id) ON DELETE CASCADE;


--
-- Name: sessions sessions_user_id_fkey; Type: FK CONSTRAINT; Schema: chat; Owner: -
--

ALTER TABLE ONLY chat.sessions
    ADD CONSTRAINT sessions_user_id_fkey FOREIGN KEY (user_id) REFERENCES folio.users(id) ON DELETE CASCADE;


--
-- Name: account_cf_class account_cf_class_account_code_fkey; Type: FK CONSTRAINT; Schema: finance; Owner: -
--

ALTER TABLE ONLY finance.account_cf_class
    ADD CONSTRAINT account_cf_class_account_code_fkey FOREIGN KEY (account_code) REFERENCES folio.chart_of_accounts(code) ON UPDATE CASCADE;


--
-- Name: account_cf_class account_cf_class_updated_by_fkey; Type: FK CONSTRAINT; Schema: finance; Owner: -
--

ALTER TABLE ONLY finance.account_cf_class
    ADD CONSTRAINT account_cf_class_updated_by_fkey FOREIGN KEY (updated_by) REFERENCES folio.users(id);


--
-- Name: ai_provider_health ai_provider_health_provider_id_fkey; Type: FK CONSTRAINT; Schema: finance; Owner: -
--

ALTER TABLE ONLY finance.ai_provider_health
    ADD CONSTRAINT ai_provider_health_provider_id_fkey FOREIGN KEY (provider_id) REFERENCES folio.ai_providers(id) ON DELETE CASCADE;


--
-- Name: ai_user_section_defaults ai_user_section_defaults_model_id_fkey; Type: FK CONSTRAINT; Schema: finance; Owner: -
--

ALTER TABLE ONLY finance.ai_user_section_defaults
    ADD CONSTRAINT ai_user_section_defaults_model_id_fkey FOREIGN KEY (model_id) REFERENCES folio.ai_models(id) ON DELETE CASCADE;


--
-- Name: ai_user_section_defaults ai_user_section_defaults_user_id_fkey; Type: FK CONSTRAINT; Schema: finance; Owner: -
--

ALTER TABLE ONLY finance.ai_user_section_defaults
    ADD CONSTRAINT ai_user_section_defaults_user_id_fkey FOREIGN KEY (user_id) REFERENCES folio.users(id) ON DELETE CASCADE;


--
-- Name: cashflow_period cashflow_period_closed_by_fkey; Type: FK CONSTRAINT; Schema: finance; Owner: -
--

ALTER TABLE ONLY finance.cashflow_period
    ADD CONSTRAINT cashflow_period_closed_by_fkey FOREIGN KEY (closed_by) REFERENCES folio.users(id);


--
-- Name: cashflow_period cashflow_period_opened_by_fkey; Type: FK CONSTRAINT; Schema: finance; Owner: -
--

ALTER TABLE ONLY finance.cashflow_period
    ADD CONSTRAINT cashflow_period_opened_by_fkey FOREIGN KEY (opened_by) REFERENCES folio.users(id);


--
-- Name: cashflow_period cashflow_period_opening_balance_journal_id_fkey; Type: FK CONSTRAINT; Schema: finance; Owner: -
--

ALTER TABLE ONLY finance.cashflow_period
    ADD CONSTRAINT cashflow_period_opening_balance_journal_id_fkey FOREIGN KEY (opening_balance_journal_id) REFERENCES folio.journal_entries(id);


--
-- Name: access_requests access_requests_actor_id_fkey; Type: FK CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.access_requests
    ADD CONSTRAINT access_requests_actor_id_fkey FOREIGN KEY (actor_id) REFERENCES folio.users(id);


--
-- Name: access_requests access_requests_resolved_by_user_id_fkey; Type: FK CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.access_requests
    ADD CONSTRAINT access_requests_resolved_by_user_id_fkey FOREIGN KEY (resolved_by_user_id) REFERENCES folio.users(id);


--
-- Name: access_requests access_requests_target_user_id_fkey; Type: FK CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.access_requests
    ADD CONSTRAINT access_requests_target_user_id_fkey FOREIGN KEY (target_user_id) REFERENCES folio.users(id);


--
-- Name: ai_assignments ai_assignments_model_id_fkey; Type: FK CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.ai_assignments
    ADD CONSTRAINT ai_assignments_model_id_fkey FOREIGN KEY (model_id) REFERENCES folio.ai_models(id) ON DELETE SET NULL;


--
-- Name: ai_assignments ai_assignments_provider_id_fkey; Type: FK CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.ai_assignments
    ADD CONSTRAINT ai_assignments_provider_id_fkey FOREIGN KEY (provider_id) REFERENCES folio.ai_providers(id) ON DELETE SET NULL;


--
-- Name: ai_assignments ai_assignments_staff_id_fkey; Type: FK CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.ai_assignments
    ADD CONSTRAINT ai_assignments_staff_id_fkey FOREIGN KEY (staff_id) REFERENCES folio.ai_staff(id) ON DELETE SET NULL;


--
-- Name: ai_chat_sessions ai_chat_sessions_user_id_fkey; Type: FK CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.ai_chat_sessions
    ADD CONSTRAINT ai_chat_sessions_user_id_fkey FOREIGN KEY (user_id) REFERENCES folio.users(id) ON DELETE CASCADE;


--
-- Name: ai_invocations ai_invocations_actor_id_fkey; Type: FK CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.ai_invocations
    ADD CONSTRAINT ai_invocations_actor_id_fkey FOREIGN KEY (actor_id) REFERENCES folio.users(id) ON DELETE SET NULL;


--
-- Name: ai_invocations ai_invocations_model_id_fkey; Type: FK CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.ai_invocations
    ADD CONSTRAINT ai_invocations_model_id_fkey FOREIGN KEY (model_id) REFERENCES folio.ai_models(id) ON DELETE SET NULL;


--
-- Name: ai_invocations ai_invocations_provider_id_fkey; Type: FK CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.ai_invocations
    ADD CONSTRAINT ai_invocations_provider_id_fkey FOREIGN KEY (provider_id) REFERENCES folio.ai_providers(id) ON DELETE SET NULL;


--
-- Name: ai_invocations ai_invocations_staff_id_fkey; Type: FK CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.ai_invocations
    ADD CONSTRAINT ai_invocations_staff_id_fkey FOREIGN KEY (staff_id) REFERENCES folio.ai_staff(id) ON DELETE SET NULL;


--
-- Name: ai_models ai_models_provider_id_fkey; Type: FK CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.ai_models
    ADD CONSTRAINT ai_models_provider_id_fkey FOREIGN KEY (provider_id) REFERENCES folio.ai_providers(id) ON DELETE CASCADE;


--
-- Name: ai_staff ai_staff_default_model_id_fkey; Type: FK CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.ai_staff
    ADD CONSTRAINT ai_staff_default_model_id_fkey FOREIGN KEY (default_model_id) REFERENCES folio.ai_models(id) ON DELETE SET NULL;


--
-- Name: ai_staff ai_staff_default_provider_id_fkey; Type: FK CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.ai_staff
    ADD CONSTRAINT ai_staff_default_provider_id_fkey FOREIGN KEY (default_provider_id) REFERENCES folio.ai_providers(id) ON DELETE SET NULL;


--
-- Name: approval_override_audit approval_override_audit_actor_id_fkey; Type: FK CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.approval_override_audit
    ADD CONSTRAINT approval_override_audit_actor_id_fkey FOREIGN KEY (actor_id) REFERENCES folio.users(id) ON DELETE SET NULL;


--
-- Name: approval_transitions approval_transitions_actor_id_fkey; Type: FK CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.approval_transitions
    ADD CONSTRAINT approval_transitions_actor_id_fkey FOREIGN KEY (actor_id) REFERENCES folio.users(id) ON DELETE SET NULL;


--
-- Name: approver_nudges approver_nudges_approver_user_id_fkey; Type: FK CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.approver_nudges
    ADD CONSTRAINT approver_nudges_approver_user_id_fkey FOREIGN KEY (approver_user_id) REFERENCES folio.users(id) ON DELETE CASCADE;


--
-- Name: customer_advisories customer_advisories_customer_id_fkey; Type: FK CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.customer_advisories
    ADD CONSTRAINT customer_advisories_customer_id_fkey FOREIGN KEY (customer_id) REFERENCES folio.customers(id) ON DELETE CASCADE;


--
-- Name: customer_contacts customer_contacts_customer_id_fkey; Type: FK CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.customer_contacts
    ADD CONSTRAINT customer_contacts_customer_id_fkey FOREIGN KEY (customer_id) REFERENCES folio.customers(id) ON DELETE CASCADE;


--
-- Name: domain_events domain_events_actor_id_fkey; Type: FK CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.domain_events
    ADD CONSTRAINT domain_events_actor_id_fkey FOREIGN KEY (actor_id) REFERENCES folio.users(id) ON DELETE SET NULL;


--
-- Name: expense_items expense_items_expense_id_fkey; Type: FK CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.expense_items
    ADD CONSTRAINT expense_items_expense_id_fkey FOREIGN KEY (expense_id) REFERENCES folio.expenses(id) ON DELETE CASCADE;


--
-- Name: expense_items expense_items_mapped_account_code_fkey; Type: FK CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.expense_items
    ADD CONSTRAINT expense_items_mapped_account_code_fkey FOREIGN KEY (mapped_account_code) REFERENCES folio.chart_of_accounts(code);


--
-- Name: expense_payments expense_payments_confirmed_by_fkey; Type: FK CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.expense_payments
    ADD CONSTRAINT expense_payments_confirmed_by_fkey FOREIGN KEY (confirmed_by) REFERENCES folio.users(id) ON DELETE RESTRICT;


--
-- Name: expense_payments expense_payments_expense_id_fkey; Type: FK CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.expense_payments
    ADD CONSTRAINT expense_payments_expense_id_fkey FOREIGN KEY (expense_id) REFERENCES folio.expenses(id) ON DELETE RESTRICT;


--
-- Name: expense_payments expense_payments_slip_id_fkey; Type: FK CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.expense_payments
    ADD CONSTRAINT expense_payments_slip_id_fkey FOREIGN KEY (slip_id) REFERENCES folio.slips(id) ON DELETE RESTRICT;


--
-- Name: expense_payments expense_payments_waybill_id_fkey; Type: FK CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.expense_payments
    ADD CONSTRAINT expense_payments_waybill_id_fkey FOREIGN KEY (waybill_id) REFERENCES folio.waybills(id) ON DELETE RESTRICT;


--
-- Name: expenses expenses_disbursed_by_fkey; Type: FK CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.expenses
    ADD CONSTRAINT expenses_disbursed_by_fkey FOREIGN KEY (disbursed_by) REFERENCES folio.users(id);


--
-- Name: expenses expenses_gl_confirmed_by_fkey; Type: FK CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.expenses
    ADD CONSTRAINT expenses_gl_confirmed_by_fkey FOREIGN KEY (gl_confirmed_by) REFERENCES folio.users(id);


--
-- Name: expenses expenses_journal_entry_id_fkey; Type: FK CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.expenses
    ADD CONSTRAINT expenses_journal_entry_id_fkey FOREIGN KEY (journal_entry_id) REFERENCES folio.journal_entries(id) ON DELETE SET NULL;


--
-- Name: expenses expenses_po_id_fkey; Type: FK CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.expenses
    ADD CONSTRAINT expenses_po_id_fkey FOREIGN KEY (po_id) REFERENCES folio.purchase_orders(id) ON DELETE SET NULL;


--
-- Name: expenses expenses_pr_id_fkey; Type: FK CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.expenses
    ADD CONSTRAINT expenses_pr_id_fkey FOREIGN KEY (pr_id) REFERENCES folio.purchase_requisitions(id) ON DELETE SET NULL;


--
-- Name: expenses expenses_rejection_actor_id_fkey; Type: FK CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.expenses
    ADD CONSTRAINT expenses_rejection_actor_id_fkey FOREIGN KEY (rejection_actor_id) REFERENCES folio.users(id);


--
-- Name: expenses expenses_submitter_id_fkey; Type: FK CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.expenses
    ADD CONSTRAINT expenses_submitter_id_fkey FOREIGN KEY (submitter_id) REFERENCES folio.users(id);


--
-- Name: slips fk_slips_po; Type: FK CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.slips
    ADD CONSTRAINT fk_slips_po FOREIGN KEY (po_id) REFERENCES folio.purchase_orders(id) ON DELETE SET NULL;


--
-- Name: slips fk_slips_pr; Type: FK CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.slips
    ADD CONSTRAINT fk_slips_pr FOREIGN KEY (pr_id) REFERENCES folio.purchase_requisitions(id) ON DELETE SET NULL;


--
-- Name: hr_leave hr_leave_employee_id_fkey; Type: FK CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.hr_leave
    ADD CONSTRAINT hr_leave_employee_id_fkey FOREIGN KEY (employee_id) REFERENCES folio.users(id);


--
-- Name: hr_leave hr_leave_waybill_id_fkey; Type: FK CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.hr_leave
    ADD CONSTRAINT hr_leave_waybill_id_fkey FOREIGN KEY (waybill_id) REFERENCES folio.waybills(id) ON DELETE CASCADE;


--
-- Name: journal_entries journal_entries_approved_by_fkey; Type: FK CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.journal_entries
    ADD CONSTRAINT journal_entries_approved_by_fkey FOREIGN KEY (approved_by) REFERENCES folio.users(id) ON DELETE SET NULL;


--
-- Name: journal_entries journal_entries_expense_id_fkey; Type: FK CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.journal_entries
    ADD CONSTRAINT journal_entries_expense_id_fkey FOREIGN KEY (expense_id) REFERENCES folio.expenses(id) ON DELETE SET NULL;


--
-- Name: journal_entries journal_entries_finalized_by_fkey; Type: FK CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.journal_entries
    ADD CONSTRAINT journal_entries_finalized_by_fkey FOREIGN KEY (finalized_by) REFERENCES folio.users(id);


--
-- Name: journal_entries journal_entries_po_id_fkey; Type: FK CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.journal_entries
    ADD CONSTRAINT journal_entries_po_id_fkey FOREIGN KEY (po_id) REFERENCES folio.purchase_orders(id) ON DELETE SET NULL;


--
-- Name: journal_entries journal_entries_pr_id_fkey; Type: FK CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.journal_entries
    ADD CONSTRAINT journal_entries_pr_id_fkey FOREIGN KEY (pr_id) REFERENCES folio.purchase_requisitions(id) ON DELETE SET NULL;


--
-- Name: journal_entries journal_entries_prepared_by_fkey; Type: FK CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.journal_entries
    ADD CONSTRAINT journal_entries_prepared_by_fkey FOREIGN KEY (prepared_by) REFERENCES folio.users(id) ON DELETE SET NULL;


--
-- Name: journal_entries journal_entries_so_id_fkey; Type: FK CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.journal_entries
    ADD CONSTRAINT journal_entries_so_id_fkey FOREIGN KEY (so_id) REFERENCES folio.sales_orders(id) ON DELETE RESTRICT;


--
-- Name: learned_mappings learned_mappings_account_code_fkey; Type: FK CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.learned_mappings
    ADD CONSTRAINT learned_mappings_account_code_fkey FOREIGN KEY (account_code) REFERENCES folio.chart_of_accounts(code);


--
-- Name: ledger_lines ledger_lines_account_code_fkey; Type: FK CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.ledger_lines
    ADD CONSTRAINT ledger_lines_account_code_fkey FOREIGN KEY (account_code) REFERENCES folio.chart_of_accounts(code);


--
-- Name: ledger_lines ledger_lines_journal_entry_id_fkey; Type: FK CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.ledger_lines
    ADD CONSTRAINT ledger_lines_journal_entry_id_fkey FOREIGN KEY (journal_entry_id) REFERENCES folio.journal_entries(id) ON DELETE CASCADE;


--
-- Name: notification_digests notification_digests_user_id_fkey; Type: FK CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.notification_digests
    ADD CONSTRAINT notification_digests_user_id_fkey FOREIGN KEY (user_id) REFERENCES folio.users(id);


--
-- Name: notifications notifications_user_id_fkey; Type: FK CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.notifications
    ADD CONSTRAINT notifications_user_id_fkey FOREIGN KEY (user_id) REFERENCES folio.users(id) ON DELETE CASCADE;

--
-- Name: notifications notifications_waybill_id_fkey; Type: FK CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.notifications
    ADD CONSTRAINT notifications_waybill_id_fkey FOREIGN KEY (waybill_id) REFERENCES folio.waybills(id) ON DELETE CASCADE;

--
-- Name: notifications notifications_event_id_fkey; Type: FK CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.notifications
    ADD CONSTRAINT notifications_event_id_fkey FOREIGN KEY (event_id) REFERENCES folio.waybill_events(id) ON DELETE SET NULL;

--
-- Name: notifications notifications_resolved_by_fkey; Type: FK CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.notifications
    ADD CONSTRAINT notifications_resolved_by_fkey FOREIGN KEY (resolved_by) REFERENCES folio.users(id) ON DELETE SET NULL;


--
-- Name: po_invoices po_invoices_draft_po_id_fkey; Type: FK CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.po_invoices
    ADD CONSTRAINT po_invoices_draft_po_id_fkey FOREIGN KEY (draft_po_id) REFERENCES folio.purchase_orders(id) ON DELETE SET NULL;


--
-- Name: po_invoices po_invoices_draft_pr_id_fkey; Type: FK CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.po_invoices
    ADD CONSTRAINT po_invoices_draft_pr_id_fkey FOREIGN KEY (draft_pr_id) REFERENCES folio.purchase_requisitions(id) ON DELETE SET NULL;


--
-- Name: po_invoices po_invoices_uploaded_by_fkey; Type: FK CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.po_invoices
    ADD CONSTRAINT po_invoices_uploaded_by_fkey FOREIGN KEY (uploaded_by) REFERENCES folio.users(id) ON DELETE SET NULL;


--
-- Name: po_invoices po_invoices_vendor_id_fkey; Type: FK CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.po_invoices
    ADD CONSTRAINT po_invoices_vendor_id_fkey FOREIGN KEY (vendor_id) REFERENCES folio.customers(id) ON DELETE SET NULL;


--
-- Name: po_items po_items_mapped_account_code_fkey; Type: FK CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.po_items
    ADD CONSTRAINT po_items_mapped_account_code_fkey FOREIGN KEY (mapped_account_code) REFERENCES folio.chart_of_accounts(code);


--
-- Name: po_items po_items_po_id_fkey; Type: FK CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.po_items
    ADD CONSTRAINT po_items_po_id_fkey FOREIGN KEY (po_id) REFERENCES folio.purchase_orders(id) ON DELETE CASCADE;


--
-- Name: policy_audit policy_audit_actor_id_fkey; Type: FK CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.policy_audit
    ADD CONSTRAINT policy_audit_actor_id_fkey FOREIGN KEY (actor_id) REFERENCES folio.users(id);


--
-- Name: policy_lint_results policy_lint_results_policy_id_fkey; Type: FK CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.policy_lint_results
    ADD CONSTRAINT policy_lint_results_policy_id_fkey FOREIGN KEY (policy_id) REFERENCES perm.policies(id) ON DELETE CASCADE;


--
-- Name: pr_items pr_items_mapped_account_code_fkey; Type: FK CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.pr_items
    ADD CONSTRAINT pr_items_mapped_account_code_fkey FOREIGN KEY (mapped_account_code) REFERENCES folio.chart_of_accounts(code);


--
-- Name: pr_items pr_items_pr_id_fkey; Type: FK CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.pr_items
    ADD CONSTRAINT pr_items_pr_id_fkey FOREIGN KEY (pr_id) REFERENCES folio.purchase_requisitions(id) ON DELETE CASCADE;


--
-- Name: purchase_orders purchase_orders_issued_by_fkey; Type: FK CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.purchase_orders
    ADD CONSTRAINT purchase_orders_issued_by_fkey FOREIGN KEY (issued_by) REFERENCES folio.users(id);


--
-- Name: purchase_orders purchase_orders_pr_id_fkey; Type: FK CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.purchase_orders
    ADD CONSTRAINT purchase_orders_pr_id_fkey FOREIGN KEY (pr_id) REFERENCES folio.purchase_requisitions(id) ON DELETE RESTRICT;


--
-- Name: purchase_orders purchase_orders_rejection_actor_id_fkey; Type: FK CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.purchase_orders
    ADD CONSTRAINT purchase_orders_rejection_actor_id_fkey FOREIGN KEY (rejection_actor_id) REFERENCES folio.users(id);


--
-- Name: purchase_orders purchase_orders_settled_by_fkey; Type: FK CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.purchase_orders
    ADD CONSTRAINT purchase_orders_settled_by_fkey FOREIGN KEY (settled_by) REFERENCES folio.users(id);


--
-- Name: purchase_orders purchase_orders_settled_slip_id_fkey; Type: FK CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.purchase_orders
    ADD CONSTRAINT purchase_orders_settled_slip_id_fkey FOREIGN KEY (settled_slip_id) REFERENCES folio.slips(id) ON DELETE SET NULL;


--
-- Name: purchase_requisitions purchase_requisitions_rejection_actor_id_fkey; Type: FK CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.purchase_requisitions
    ADD CONSTRAINT purchase_requisitions_rejection_actor_id_fkey FOREIGN KEY (rejection_actor_id) REFERENCES folio.users(id);


--
-- Name: purchase_requisitions purchase_requisitions_requester_id_fkey; Type: FK CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.purchase_requisitions
    ADD CONSTRAINT purchase_requisitions_requester_id_fkey FOREIGN KEY (requester_id) REFERENCES folio.users(id);


--
-- Name: sales_orders sales_orders_customer_id_fkey; Type: FK CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.sales_orders
    ADD CONSTRAINT sales_orders_customer_id_fkey FOREIGN KEY (customer_id) REFERENCES folio.customers(id) ON DELETE RESTRICT;


--
-- Name: sales_orders sales_orders_invoice_issuer_id_fkey; Type: FK CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.sales_orders
    ADD CONSTRAINT sales_orders_invoice_issuer_id_fkey FOREIGN KEY (invoice_issuer_id) REFERENCES folio.users(id);


--
-- Name: sales_orders sales_orders_paid_by_id_fkey; Type: FK CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.sales_orders
    ADD CONSTRAINT sales_orders_paid_by_id_fkey FOREIGN KEY (paid_by_id) REFERENCES folio.users(id);


--
-- Name: sales_orders sales_orders_rejection_actor_id_fkey; Type: FK CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.sales_orders
    ADD CONSTRAINT sales_orders_rejection_actor_id_fkey FOREIGN KEY (rejection_actor_id) REFERENCES folio.users(id);


--
-- Name: sales_orders sales_orders_sales_rep_id_fkey; Type: FK CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.sales_orders
    ADD CONSTRAINT sales_orders_sales_rep_id_fkey FOREIGN KEY (sales_rep_id) REFERENCES folio.users(id) ON DELETE RESTRICT;


--
-- Name: sales_product_embeddings sales_product_embeddings_so_item_id_fkey; Type: FK CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.sales_product_embeddings
    ADD CONSTRAINT sales_product_embeddings_so_item_id_fkey FOREIGN KEY (so_item_id) REFERENCES folio.so_items(id) ON DELETE CASCADE;


--
-- Name: slips slips_discarded_by_fkey; Type: FK CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.slips
    ADD CONSTRAINT slips_discarded_by_fkey FOREIGN KEY (discarded_by) REFERENCES folio.users(id);


--
-- Name: slips slips_expense_id_fkey; Type: FK CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.slips
    ADD CONSTRAINT slips_expense_id_fkey FOREIGN KEY (expense_id) REFERENCES folio.expenses(id) ON DELETE SET NULL;


--
-- Name: slips slips_uploaded_by_fkey; Type: FK CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.slips
    ADD CONSTRAINT slips_uploaded_by_fkey FOREIGN KEY (uploaded_by) REFERENCES folio.users(id);


--
-- Name: so_items so_items_mapped_revenue_account_code_fkey; Type: FK CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.so_items
    ADD CONSTRAINT so_items_mapped_revenue_account_code_fkey FOREIGN KEY (mapped_revenue_account_code) REFERENCES folio.chart_of_accounts(code);


--
-- Name: so_items so_items_sales_order_id_fkey; Type: FK CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.so_items
    ADD CONSTRAINT so_items_sales_order_id_fkey FOREIGN KEY (sales_order_id) REFERENCES folio.sales_orders(id) ON DELETE CASCADE;


--
-- Name: vendor_embeddings vendor_embeddings_expense_id_fkey; Type: FK CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.vendor_embeddings
    ADD CONSTRAINT vendor_embeddings_expense_id_fkey FOREIGN KEY (expense_id) REFERENCES folio.expenses(id) ON DELETE CASCADE;


--
-- Name: vendor_embeddings vendor_embeddings_submitter_id_fkey; Type: FK CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.vendor_embeddings
    ADD CONSTRAINT vendor_embeddings_submitter_id_fkey FOREIGN KEY (submitter_id) REFERENCES folio.users(id) ON DELETE SET NULL;


--
-- Name: waybill_attachments waybill_attachments_waybill_id_fkey; Type: FK CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.waybill_attachments
    ADD CONSTRAINT waybill_attachments_waybill_id_fkey FOREIGN KEY (waybill_id) REFERENCES folio.waybills(id) ON DELETE RESTRICT;


--
-- Name: waybill_events waybill_events_previous_event_id_fkey; Type: FK CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.waybill_events
    ADD CONSTRAINT waybill_events_previous_event_id_fkey FOREIGN KEY (previous_event_id) REFERENCES folio.waybill_events(id);


--
-- Name: waybill_events waybill_events_waybill_id_fkey; Type: FK CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.waybill_events
    ADD CONSTRAINT waybill_events_waybill_id_fkey FOREIGN KEY (waybill_id) REFERENCES folio.waybills(id) ON DELETE RESTRICT;


--
-- Name: waybill_stage_claims waybill_stage_claims_claimed_by_fkey; Type: FK CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.waybill_stage_claims
    ADD CONSTRAINT waybill_stage_claims_claimed_by_fkey FOREIGN KEY (claimed_by) REFERENCES folio.users(id) ON DELETE RESTRICT;


--
-- Name: waybill_stage_claims waybill_stage_claims_released_by_fkey; Type: FK CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.waybill_stage_claims
    ADD CONSTRAINT waybill_stage_claims_released_by_fkey FOREIGN KEY (released_by) REFERENCES folio.users(id) ON DELETE SET NULL;


--
-- Name: waybill_stage_claims waybill_stage_claims_waybill_id_fkey; Type: FK CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.waybill_stage_claims
    ADD CONSTRAINT waybill_stage_claims_waybill_id_fkey FOREIGN KEY (waybill_id) REFERENCES folio.waybills(id) ON DELETE RESTRICT;


--
-- Name: waybill_watchers waybill_watchers_user_id_fkey; Type: FK CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.waybill_watchers
    ADD CONSTRAINT waybill_watchers_user_id_fkey FOREIGN KEY (user_id) REFERENCES folio.users(id) ON DELETE CASCADE;


--
-- Name: waybill_watchers waybill_watchers_waybill_id_fkey; Type: FK CONSTRAINT; Schema: folio; Owner: -
--

ALTER TABLE ONLY folio.waybill_watchers
    ADD CONSTRAINT waybill_watchers_waybill_id_fkey FOREIGN KEY (waybill_id) REFERENCES folio.waybills(id) ON DELETE CASCADE;


--
-- Name: hook_events hook_events_provider_id_fkey; Type: FK CONSTRAINT; Schema: hook; Owner: -
--

ALTER TABLE ONLY hook.hook_events
    ADD CONSTRAINT hook_events_provider_id_fkey FOREIGN KEY (provider_id) REFERENCES hook.hook_providers(id);


--
-- Name: contract_chunks contract_chunks_contract_id_fkey; Type: FK CONSTRAINT; Schema: law; Owner: -
--

ALTER TABLE ONLY law.contract_chunks
    ADD CONSTRAINT contract_chunks_contract_id_fkey FOREIGN KEY (contract_id) REFERENCES law.contracts(id) ON DELETE CASCADE;


--
-- Name: contract_pages contract_pages_contract_id_fkey; Type: FK CONSTRAINT; Schema: law; Owner: -
--

ALTER TABLE ONLY law.contract_pages
    ADD CONSTRAINT contract_pages_contract_id_fkey FOREIGN KEY (contract_id) REFERENCES law.contracts(id) ON DELETE CASCADE;


--
-- Name: department_permissions department_permissions_department_id_fkey; Type: FK CONSTRAINT; Schema: perm; Owner: -
--

ALTER TABLE ONLY perm.department_permissions
    ADD CONSTRAINT department_permissions_department_id_fkey FOREIGN KEY (department_id) REFERENCES perm.departments(id) ON DELETE CASCADE;


--
-- Name: department_permissions department_permissions_permission_id_fkey; Type: FK CONSTRAINT; Schema: perm; Owner: -
--

ALTER TABLE ONLY perm.department_permissions
    ADD CONSTRAINT department_permissions_permission_id_fkey FOREIGN KEY (permission_id) REFERENCES perm.permissions(id) ON DELETE CASCADE;


--
-- Name: departments departments_head_user_id_fkey; Type: FK CONSTRAINT; Schema: perm; Owner: -
--

ALTER TABLE ONLY perm.departments
    ADD CONSTRAINT departments_head_user_id_fkey FOREIGN KEY (head_user_id) REFERENCES folio.users(id) ON DELETE SET NULL;


--
-- Name: role_permissions role_permissions_permission_id_fkey; Type: FK CONSTRAINT; Schema: perm; Owner: -
--

ALTER TABLE ONLY perm.role_permissions
    ADD CONSTRAINT role_permissions_permission_id_fkey FOREIGN KEY (permission_id) REFERENCES perm.permissions(id) ON DELETE CASCADE;


--
-- Name: role_permissions role_permissions_role_id_role_kind_fkey; Type: FK CONSTRAINT; Schema: perm; Owner: -
--

ALTER TABLE ONLY perm.role_permissions
    ADD CONSTRAINT role_permissions_role_id_role_kind_fkey FOREIGN KEY (role_id, role_kind) REFERENCES perm.roles(id, kind) ON DELETE CASCADE;


--
-- Name: roles roles_department_id_fkey; Type: FK CONSTRAINT; Schema: perm; Owner: -
--

ALTER TABLE ONLY perm.roles
    ADD CONSTRAINT roles_department_id_fkey FOREIGN KEY (department_id) REFERENCES perm.departments(id) ON DELETE RESTRICT;


--
-- Name: user_departments user_departments_assigned_by_fkey; Type: FK CONSTRAINT; Schema: perm; Owner: -
--

ALTER TABLE ONLY perm.user_departments
    ADD CONSTRAINT user_departments_assigned_by_fkey FOREIGN KEY (assigned_by) REFERENCES folio.users(id) ON DELETE SET NULL;


--
-- Name: user_departments user_departments_department_id_fkey; Type: FK CONSTRAINT; Schema: perm; Owner: -
--

ALTER TABLE ONLY perm.user_departments
    ADD CONSTRAINT user_departments_department_id_fkey FOREIGN KEY (department_id) REFERENCES perm.departments(id) ON DELETE RESTRICT;


--
-- Name: user_departments user_departments_user_id_fkey; Type: FK CONSTRAINT; Schema: perm; Owner: -
--

ALTER TABLE ONLY perm.user_departments
    ADD CONSTRAINT user_departments_user_id_fkey FOREIGN KEY (user_id) REFERENCES folio.users(id) ON DELETE CASCADE;


--
-- Name: user_permissions user_permissions_permission_id_fkey; Type: FK CONSTRAINT; Schema: perm; Owner: -
--

ALTER TABLE ONLY perm.user_permissions
    ADD CONSTRAINT user_permissions_permission_id_fkey FOREIGN KEY (permission_id) REFERENCES perm.permissions(id) ON DELETE CASCADE;


--
-- Name: user_permissions user_permissions_user_id_fkey; Type: FK CONSTRAINT; Schema: perm; Owner: -
--

ALTER TABLE ONLY perm.user_permissions
    ADD CONSTRAINT user_permissions_user_id_fkey FOREIGN KEY (user_id) REFERENCES folio.users(id) ON DELETE CASCADE;


--
-- Name: user_roles user_roles_role_id_role_kind_fkey; Type: FK CONSTRAINT; Schema: perm; Owner: -
--

ALTER TABLE ONLY perm.user_roles
    ADD CONSTRAINT user_roles_role_id_role_kind_fkey FOREIGN KEY (role_id, role_kind) REFERENCES perm.roles(id, kind) ON DELETE RESTRICT;


--
-- Name: user_roles user_roles_user_id_fkey; Type: FK CONSTRAINT; Schema: perm; Owner: -
--

ALTER TABLE ONLY perm.user_roles
    ADD CONSTRAINT user_roles_user_id_fkey FOREIGN KEY (user_id) REFERENCES folio.users(id) ON DELETE CASCADE;


--
-- Name: messages chat_messages_self; Type: POLICY; Schema: chat; Owner: -
--

CREATE POLICY chat_messages_self ON chat.messages USING ((session_id IN ( SELECT sessions.id
   FROM chat.sessions
  WHERE (sessions.user_id = (current_setting('app.user_id'::text, true))::integer))));


--
-- Name: sessions chat_sessions_self; Type: POLICY; Schema: chat; Owner: -
--

CREATE POLICY chat_sessions_self ON chat.sessions USING ((user_id = (current_setting('app.user_id'::text, true))::integer));


--
-- Name: messages; Type: ROW SECURITY; Schema: chat; Owner: -
--

ALTER TABLE chat.messages ENABLE ROW LEVEL SECURITY;

--
-- Name: sessions; Type: ROW SECURITY; Schema: chat; Owner: -
--

ALTER TABLE chat.sessions ENABLE ROW LEVEL SECURITY;

REVOKE UPDATE, DELETE ON folio.waybill_events FROM CURRENT_USER;

DO $do$
BEGIN
  IF EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'n8n_user') THEN
    EXECUTE 'REVOKE UPDATE, DELETE ON folio.waybill_events FROM n8n_user';
  END IF;

  IF EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'folio_readonly_agent') THEN
    EXECUTE 'GRANT USAGE ON SCHEMA finance, folio, law, perm TO folio_readonly_agent';
    EXECUTE 'GRANT SELECT ON ALL TABLES IN SCHEMA finance, folio, law, perm TO folio_readonly_agent';
  END IF;
END
$do$;

SELECT format(
  'ALTER DATABASE %I SET search_path TO finance, perm, hook, law, n8n, folio, public',
  current_database()
) \gexec

COMMIT;
