-- folio/db/2026-07-13-procurement-gl-2step.sql
--
-- 2-step GL for procurement waybills (PR / PO):
--   Step 1: accrual      (Dr expense/VAT, Cr accounts-payable)  -- committed at accounting_authorization
--   Step 2: settlement   (Dr accounts-payable, Cr cash-at-bank) -- recorded at disbursed
--
-- Expense keeps a single 'reimbursement' step (existing behavior).
-- PR/PO can have up to one draft per (origin_id, step).
--
-- Also extends waybill_events.kind to record the new GL posts/confirmations.

BEGIN;

ALTER TABLE journal_entries
    ADD COLUMN IF NOT EXISTS step text NOT NULL DEFAULT 'reimbursement';

ALTER TABLE journal_entries
    DROP CONSTRAINT IF EXISTS journal_entries_step_check;
ALTER TABLE journal_entries
    ADD CONSTRAINT journal_entries_step_check
    CHECK (step IN ('reimbursement', 'accrual', 'settlement'));

DROP INDEX IF EXISTS idx_journal_entries_one_draft_per_expense;
CREATE UNIQUE INDEX IF NOT EXISTS idx_journal_entries_one_draft_per_expense
    ON journal_entries(expense_id, step)
    WHERE is_draft = TRUE AND expense_id IS NOT NULL;

CREATE UNIQUE INDEX IF NOT EXISTS idx_journal_entries_one_draft_per_pr
    ON journal_entries(pr_id, step)
    WHERE is_draft = TRUE AND pr_id IS NOT NULL;

CREATE UNIQUE INDEX IF NOT EXISTS idx_journal_entries_one_draft_per_po
    ON journal_entries(po_id, step)
    WHERE is_draft = TRUE AND po_id IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_journal_entries_pr_step
    ON journal_entries(pr_id, step);
CREATE INDEX IF NOT EXISTS idx_journal_entries_po_step
    ON journal_entries(po_id, step);

ALTER TABLE journal_entries
    DROP CONSTRAINT IF EXISTS journal_entries_draft_source_check;
ALTER TABLE journal_entries
    ADD CONSTRAINT journal_entries_draft_source_check
    CHECK (draft_source IN ('expense', 'pr', 'po'));

ALTER TABLE waybill_events
    DROP CONSTRAINT IF EXISTS waybill_events_kind_check;
ALTER TABLE waybill_events
    ADD CONSTRAINT waybill_events_kind_check
    CHECK (kind = ANY (ARRAY[
        'created','submitted','advanced','rejected','corrected',
        'settled','posted-to-gl','gl-confirmed','slip-attached','attached',
        'signed-off','reversed','authorization-overridden','resubmitted','superseded',
        'posted-to-gl-accrual','gl-confirmed-accrual',
        'posted-to-gl-settlement','gl-confirmed-settlement',
        'created-draft-gl-accrual','created-draft-gl-settlement'
    ]));

COMMIT;
