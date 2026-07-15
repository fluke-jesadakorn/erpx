-- db/2026-07-09-A-stage-rename-and-waybill-foundation.sql
--
-- 1. Rename legacy snake_case approval stage codes to finance-standard keys.
-- 2. Create `waybills` + `waybill_events` + sequence function.
-- 3. Backfill waybills from existing expenses / purchase_requisitions.
--
-- Run with:
--   PGPASSWORD=contractpw psql -h localhost -U contract -d folio_db \
--     -v ON_ERROR_STOP=1 -f folio/db/2026-07-09-A-stage-rename-and-waybill-foundation.sql

BEGIN;

-- ============================================================================
-- 1. Stage rename: snake_case → finance-standard keys
-- ============================================================================

-- expenses.status
UPDATE expenses SET status = 'submission'               WHERE status = 'ocr_extracted';
UPDATE expenses SET status = 'dept_verification'         WHERE status = 'supervisor_review';
UPDATE expenses SET status = 'dept_authorization'        WHERE status = 'manager_review';
UPDATE expenses SET status = 'accounting_verification'   WHERE status = 'account_officer_review';
UPDATE expenses SET status = 'accounting_supervision'    WHERE status = 'account_supervisor_review';
UPDATE expenses SET status = 'accounting_authorization'  WHERE status = 'accounting_review';
UPDATE expenses SET status = 'disbursement_authorization' WHERE status = 'finance_review';
UPDATE expenses SET status = 'cfo_authorization'         WHERE status = 'cfo_review';
UPDATE expenses SET status = 'ceo_authorization'         WHERE status = 'ceo_review';
UPDATE expenses SET status = 'awaiting_disbursement'     WHERE status = 'approved';
UPDATE expenses SET status = 'disbursed'                 WHERE status = 'paid';

-- purchase_requisitions.status (legacy codes apply)
UPDATE purchase_requisitions SET status = 'submission'               WHERE status = 'ocr_extracted';
UPDATE purchase_requisitions SET status = 'dept_verification'         WHERE status = 'supervisor_review';
UPDATE purchase_requisitions SET status = 'dept_authorization'        WHERE status = 'manager_review';
UPDATE purchase_requisitions SET status = 'accounting_verification'   WHERE status = 'account_officer_review';
UPDATE purchase_requisitions SET status = 'accounting_supervision'    WHERE status = 'account_supervisor_review';
UPDATE purchase_requisitions SET status = 'accounting_authorization'  WHERE status = 'accounting_review';
UPDATE purchase_requisitions SET status = 'disbursement_authorization' WHERE status = 'finance_review';
UPDATE purchase_requisitions SET status = 'cfo_authorization'         WHERE status = 'cfo_review';
UPDATE purchase_requisitions SET status = 'ceo_authorization'         WHERE status = 'ceo_review';
UPDATE purchase_requisitions SET status = 'awaiting_disbursement'     WHERE status = 'approved';
UPDATE purchase_requisitions SET status = 'disbursed'                 WHERE status = 'paid';

-- purchase_orders.status (PO uses its own status values; map to finance-standard)
UPDATE purchase_orders SET status = 'submission'               WHERE status = 'draft';
UPDATE purchase_orders SET status = 'dept_verification'         WHERE status = 'manager_review';
UPDATE purchase_orders SET status = 'accounting_verification'   WHERE status = 'accounting_review';
UPDATE purchase_orders SET status = 'cfo_authorization'         WHERE status = 'pending_approval' OR status = 'po_cfo';
UPDATE purchase_orders SET status = 'awaiting_disbursement'     WHERE status = 'approved';
UPDATE purchase_orders SET status = 'disbursed'                 WHERE status = 'settled';

-- approval_transitions: rename stage/previous_status/new_status to new keys
UPDATE approval_transitions SET stage = 'submission'               WHERE stage = 'ocr_extracted';
UPDATE approval_transitions SET stage = 'dept_verification'         WHERE stage = 'supervisor_review';
UPDATE approval_transitions SET stage = 'dept_authorization'        WHERE stage = 'manager_review';
UPDATE approval_transitions SET stage = 'accounting_verification'   WHERE stage = 'account_officer_review';
UPDATE approval_transitions SET stage = 'accounting_supervision'    WHERE stage = 'account_supervisor_review';
UPDATE approval_transitions SET stage = 'accounting_authorization'  WHERE stage = 'accounting_review';
UPDATE approval_transitions SET stage = 'disbursement_authorization' WHERE stage = 'finance_review';
UPDATE approval_transitions SET stage = 'cfo_authorization'         WHERE stage = 'cfo_review';
UPDATE approval_transitions SET stage = 'ceo_authorization'         WHERE stage = 'ceo_review';
UPDATE approval_transitions SET stage = 'awaiting_disbursement'     WHERE stage = 'approved';
UPDATE approval_transitions SET stage = 'disbursed'                 WHERE stage = 'paid';

UPDATE approval_transitions SET previous_status = 'submission'               WHERE previous_status = 'ocr_extracted';
UPDATE approval_transitions SET previous_status = 'dept_verification'         WHERE previous_status = 'supervisor_review';
UPDATE approval_transitions SET previous_status = 'dept_authorization'        WHERE previous_status = 'manager_review';
UPDATE approval_transitions SET previous_status = 'accounting_verification'   WHERE previous_status = 'account_officer_review';
UPDATE approval_transitions SET previous_status = 'accounting_supervision'    WHERE previous_status = 'account_supervisor_review';
UPDATE approval_transitions SET previous_status = 'accounting_authorization'  WHERE previous_status = 'accounting_review';
UPDATE approval_transitions SET previous_status = 'disbursement_authorization' WHERE previous_status = 'finance_review';
UPDATE approval_transitions SET previous_status = 'cfo_authorization'         WHERE previous_status = 'cfo_review';
UPDATE approval_transitions SET previous_status = 'ceo_authorization'         WHERE previous_status = 'ceo_review';
UPDATE approval_transitions SET previous_status = 'awaiting_disbursement'     WHERE previous_status = 'approved';
UPDATE approval_transitions SET previous_status = 'disbursed'                 WHERE previous_status = 'paid';

UPDATE approval_transitions SET new_status = 'submission'               WHERE new_status = 'ocr_extracted';
UPDATE approval_transitions SET new_status = 'dept_verification'         WHERE new_status = 'supervisor_review';
UPDATE approval_transitions SET new_status = 'dept_authorization'        WHERE new_status = 'manager_review';
UPDATE approval_transitions SET new_status = 'accounting_verification'   WHERE new_status = 'account_officer_review';
UPDATE approval_transitions SET new_status = 'accounting_supervision'    WHERE new_status = 'account_supervisor_review';
UPDATE approval_transitions SET new_status = 'accounting_authorization'  WHERE new_status = 'accounting_review';
UPDATE approval_transitions SET new_status = 'disbursement_authorization' WHERE new_status = 'finance_review';
UPDATE approval_transitions SET new_status = 'cfo_authorization'         WHERE new_status = 'cfo_review';
UPDATE approval_transitions SET new_status = 'ceo_authorization'         WHERE new_status = 'ceo_review';
UPDATE approval_transitions SET new_status = 'awaiting_disbursement'     WHERE new_status = 'approved';
UPDATE approval_transitions SET new_status = 'disbursed'                 WHERE new_status = 'paid';

-- ============================================================================
-- 2. CHECK constraints: enforce finance-standard keys
-- ============================================================================

ALTER TABLE expenses DROP CONSTRAINT IF EXISTS expenses_status_chk;
ALTER TABLE expenses ADD CONSTRAINT expenses_status_chk CHECK (status IN (
  'draft','submission','dept_verification','dept_authorization',
  'accounting_verification','accounting_supervision','accounting_authorization',
  'disbursement_authorization','cfo_authorization','ceo_authorization',
  'awaiting_disbursement','disbursed','rejected'
));

ALTER TABLE purchase_requisitions DROP CONSTRAINT IF EXISTS purchase_requisitions_status_chk;
ALTER TABLE purchase_requisitions ADD CONSTRAINT purchase_requisitions_status_chk CHECK (status IN (
  'draft','submission','dept_verification','dept_authorization',
  'accounting_verification','accounting_supervision','accounting_authorization',
  'disbursement_authorization','cfo_authorization','ceo_authorization',
  'awaiting_disbursement','disbursed','rejected'
));

ALTER TABLE purchase_orders DROP CONSTRAINT IF EXISTS purchase_orders_status_chk;
ALTER TABLE purchase_orders ADD CONSTRAINT purchase_orders_status_chk CHECK (status IN (
  'draft','submission','dept_verification','dept_authorization',
  'accounting_verification','accounting_supervision','accounting_authorization',
  'disbursement_authorization','cfo_authorization','ceo_authorization',
  'awaiting_disbursement','disbursed','rejected'
));

-- ============================================================================
-- 3. Waybill foundation
-- ============================================================================

CREATE TABLE IF NOT EXISTS waybills (
  id                      text PRIMARY KEY,
  origin                  text NOT NULL CHECK (origin IN ('expense','pr','po')),
  origin_id               int  NOT NULL,
  fiscal_year             smallint NOT NULL,
  waybill_kind            text NOT NULL CHECK (waybill_kind IN ('reimbursement','procurement')),
  submitter_id            int,
  vendor_name             text,
  total_amount            numeric(14,2),
  currency                text NOT NULL DEFAULT 'THB',
  current_stage           text NOT NULL,
  current_owner_role      text,
  current_owner_user_id   int,
  status                  text NOT NULL DEFAULT 'open'
                          CHECK (status IN ('open','completed','rejected','reversed','superseded')),
  created_at              timestamptz NOT NULL DEFAULT now(),
  updated_at              timestamptz NOT NULL DEFAULT now(),
  UNIQUE (origin, origin_id)
);

CREATE INDEX IF NOT EXISTS idx_waybills_origin         ON waybills (origin, origin_id);
CREATE INDEX IF NOT EXISTS idx_waybills_stage          ON waybills (current_stage);
CREATE INDEX IF NOT EXISTS idx_waybills_owner          ON waybills (current_owner_user_id);
CREATE INDEX IF NOT EXISTS idx_waybills_submitter      ON waybills (submitter_id);
CREATE INDEX IF NOT EXISTS idx_waybills_fy             ON waybills (fiscal_year);

CREATE TABLE IF NOT EXISTS waybill_events (
  id                  uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  waybill_id          text NOT NULL REFERENCES waybills(id) ON DELETE RESTRICT,
  sequence            int  NOT NULL,
  previous_event_id   uuid REFERENCES waybill_events(id),
  kind                text NOT NULL CHECK (kind IN (
                        'submitted','advanced','rejected','corrected',
                        'settled','posted-to-gl','slip-attached',
                        'signed-off','reversed','authorization-overridden',
                        'resubmitted','superseded','created')),
  stage_from          text,
  stage_to            text,
  actor_id            int,
  actor_role          text,
  actor_signature     bytea,
  occurred_at         timestamptz NOT NULL DEFAULT now(),
  payload             jsonb,
  CHECK (sequence >= 1)
);

CREATE INDEX IF NOT EXISTS idx_wbx_waybill          ON waybill_events (waybill_id, sequence);
CREATE INDEX IF NOT EXISTS idx_wbx_kind             ON waybill_events (kind);
CREATE INDEX IF NOT EXISTS idx_wbx_previous         ON waybill_events (previous_event_id);
CREATE INDEX IF NOT EXISTS idx_wbx_actor            ON waybill_events (actor_id);

-- Append-only: revoke UPDATE/DELETE from app roles.
DO $$
BEGIN
  IF EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'n8n_user') THEN
    EXECUTE 'REVOKE UPDATE, DELETE ON waybill_events FROM n8n_user';
  END IF;
  IF EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'contract') THEN
    EXECUTE 'REVOKE UPDATE, DELETE ON waybill_events FROM contract';
  END IF;
END
$$;

-- ============================================================================
-- 4. Waybill sequence generator
-- ============================================================================

CREATE OR REPLACE FUNCTION next_waybill_number(p_fiscal_year smallint)
RETURNS text
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

GRANT EXECUTE ON FUNCTION next_waybill_number(smallint) TO contract, n8n_user;

-- ============================================================================
-- 5. Backfill waybills from existing expenses + PRs + POs
-- ============================================================================

INSERT INTO waybills (
  id, origin, origin_id, fiscal_year, waybill_kind,
  submitter_id, vendor_name, total_amount, currency,
  current_stage, status, created_at, updated_at
)
SELECT
  next_waybill_number(EXTRACT(YEAR FROM e.created_at)::smallint),
  'expense',
  e.id,
  EXTRACT(YEAR FROM e.created_at)::smallint,
  'reimbursement',
  e.submitter_id,
  e.vendor_name,
  e.total_amount,
  COALESCE(e.payment_method, 'THB'),
  e.status,
  CASE
    WHEN e.status = 'disbursed'                    THEN 'completed'
    WHEN e.status IN ('rejected')                  THEN 'rejected'
    ELSE 'open'
  END,
  e.created_at,
  e.updated_at
FROM expenses e
WHERE NOT EXISTS (
  SELECT 1 FROM waybills w WHERE w.origin = 'expense' AND w.origin_id = e.id
);

INSERT INTO waybills (
  id, origin, origin_id, fiscal_year, waybill_kind,
  submitter_id, vendor_name, total_amount, currency,
  current_stage, status, created_at, updated_at
)
SELECT
  next_waybill_number(EXTRACT(YEAR FROM p.created_at)::smallint),
  'pr',
  p.id,
  EXTRACT(YEAR FROM p.created_at)::smallint,
  'procurement',
  p.requester_id,
  p.vendor_name,
  p.total_estimate,
  COALESCE(p.currency, 'THB'),
  p.status,
  CASE
    WHEN p.status = 'disbursed' THEN 'completed'
    WHEN p.status = 'rejected' THEN 'rejected'
    ELSE 'open'
  END,
  p.created_at,
  p.updated_at
FROM purchase_requisitions p
WHERE NOT EXISTS (
  SELECT 1 FROM waybills w WHERE w.origin = 'pr' AND w.origin_id = p.id
);

INSERT INTO waybills (
  id, origin, origin_id, fiscal_year, waybill_kind,
  submitter_id, vendor_name, total_amount, currency,
  current_stage, status, created_at, updated_at
)
SELECT
  next_waybill_number(EXTRACT(YEAR FROM o.created_at)::smallint),
  'po',
  o.id,
  EXTRACT(YEAR FROM o.created_at)::smallint,
  'procurement',
  o.issued_by,
  o.vendor_name,
  o.total_amount,
  COALESCE(o.currency, 'THB'),
  o.status,
  CASE
    WHEN o.status = 'disbursed' THEN 'completed'
    WHEN o.status = 'rejected' THEN 'rejected'
    ELSE 'open'
  END,
  o.created_at,
  o.updated_at
FROM purchase_orders o
WHERE NOT EXISTS (
  SELECT 1 FROM waybills w WHERE w.origin = 'po' AND w.origin_id = o.id
);

-- Set current_owner_role + current_owner_user_id from latest approval_transitions row
-- where stage matches the current_stage of the waybill.

-- ============================================================================
-- 6. Audit log: append a 'created' event for every backfilled waybill
--    so the linked-list is non-empty from day 1.
-- ============================================================================

INSERT INTO waybill_events (waybill_id, sequence, kind, stage_to, occurred_at, payload)
SELECT
  w.id,
  1,
  'created',
  w.current_stage,
  w.created_at,
  jsonb_build_object('backfill', true, 'origin', w.origin, 'origin_id', w.origin_id)
FROM waybills w
WHERE NOT EXISTS (
  SELECT 1 FROM waybill_events e WHERE e.waybill_id = w.id
);

-- ============================================================================
-- 7. Audit row
-- ============================================================================

INSERT INTO perm.audit (kind, actor, target)
VALUES (
  'schema.migration',
  'system',
  jsonb_build_object(
    'migration', '2026-07-09-A-stage-rename-and-waybill-foundation',
    'description', 'Renamed stage codes to finance-standard + created waybills + waybill_events + sequence'
  )
);

COMMIT;
