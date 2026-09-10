PRAGMA defer_foreign_keys = ON;

-- Keep the owning record and all existing foreign keys/IDs. Only drafts may be incomplete.
CREATE TABLE finance_expenses_next (
  id TEXT PRIMARY KEY, company_id TEXT NOT NULL, employee_id TEXT,
  expense_date TEXT, merchant TEXT, currency TEXT,
  amount_minor INTEGER CHECK(amount_minor IS NULL OR amount_minor > 0),
  tax_minor INTEGER NOT NULL DEFAULT 0 CHECK(tax_minor >= 0),
  category_account_id TEXT REFERENCES finance_accounts(id),
  status TEXT NOT NULL DEFAULT 'draft' CHECK(status IN ('draft','in_review','approved','rejected','posted','reimbursed')),
  journal_id TEXT REFERENCES finance_journals(id),
  evidence_json TEXT NOT NULL DEFAULT '[]' CHECK(json_valid(evidence_json)),
  version INTEGER NOT NULL DEFAULT 1, created_by TEXT NOT NULL, approved_by TEXT,
  created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL,
  branch_id TEXT REFERENCES organization_nodes(id), department_id TEXT REFERENCES organization_nodes(id),
  engagement_id TEXT REFERENCES hr_engagements(id), business_purpose TEXT NOT NULL DEFAULT '',
  receipt_reference TEXT, rejection_reason TEXT, prepared_by TEXT, submitted_at INTEGER,
  posted_at INTEGER, reimbursed_at INTEGER, cancelled_at INTEGER,
  invoice_id TEXT REFERENCES finance_invoices(id), reversal_journal_id TEXT REFERENCES finance_journals(id),
  claimant_user_id TEXT, claimant_name TEXT,
  workflow_version INTEGER NOT NULL DEFAULT 1 CHECK(workflow_version IN (1,2)),
  workflow_stage TEXT NOT NULL DEFAULT 'draft' CHECK(workflow_stage IN ('draft','department_approval','accounts_review','accounts_approval','payment','completed','cancelled')),
  accounting_json TEXT NOT NULL DEFAULT '{}' CHECK(json_valid(accounting_json)),
  CHECK(workflow_version=1 OR status IN ('draft','rejected') OR
    (claimant_user_id IS NOT NULL AND employee_id IS NOT NULL AND engagement_id IS NOT NULL
    AND department_id IS NOT NULL AND expense_date IS NOT NULL AND length(merchant)>=2
    AND currency IS NOT NULL AND amount_minor IS NOT NULL AND category_account_id IS NOT NULL
    AND length(business_purpose)>=5 AND length(receipt_reference)>=2))
) STRICT;
INSERT INTO finance_expenses_next (
 id,company_id,employee_id,expense_date,merchant,currency,amount_minor,tax_minor,category_account_id,
 status,journal_id,evidence_json,version,created_by,approved_by,created_at,updated_at,branch_id,department_id,
 engagement_id,business_purpose,receipt_reference,rejection_reason,prepared_by,submitted_at,posted_at,
 reimbursed_at,cancelled_at,invoice_id,reversal_journal_id,claimant_user_id,claimant_name,workflow_stage
)
SELECT x.id,x.company_id,x.employee_id,x.expense_date,x.merchant,x.currency,x.amount_minor,x.tax_minor,x.category_account_id,
 x.status,x.journal_id,x.evidence_json,x.version,x.created_by,x.approved_by,x.created_at,x.updated_at,x.branch_id,x.department_id,
 x.engagement_id,x.business_purpose,x.receipt_reference,x.rejection_reason,x.prepared_by,x.submitted_at,x.posted_at,
 x.reimbursed_at,x.cancelled_at,x.invoice_id,x.reversal_journal_id,COALESCE(p.user_id,x.created_by),p.legal_name,
 CASE WHEN x.cancelled_at IS NOT NULL THEN 'cancelled' WHEN x.status='reimbursed' THEN 'completed'
 WHEN x.status='posted' THEN 'payment' WHEN x.status='approved' THEN 'accounts_approval'
 WHEN x.status='in_review' THEN 'department_approval' ELSE 'draft' END
FROM finance_expenses x LEFT JOIN hr_people p ON p.id=x.employee_id;
-- Rebuild the dependent table in the same transaction. D1 tracks deferred
-- violations across DROP/RENAME even when the replacement keeps the same IDs.
CREATE TABLE expense_decisions_next (
  id TEXT PRIMARY KEY,
  expense_id TEXT NOT NULL REFERENCES finance_expenses_next(id),
  command_id TEXT NOT NULL UNIQUE REFERENCES command_receipts(command_id),
  action TEXT NOT NULL CHECK(action IN ('approve','reject','cancel')),
  actor_id TEXT NOT NULL, reviewed_version INTEGER NOT NULL,
  reason TEXT NOT NULL, created_at INTEGER NOT NULL
) STRICT;
INSERT INTO expense_decisions_next SELECT * FROM expense_decisions;
DROP TABLE expense_decisions;
DROP TABLE finance_expenses;
ALTER TABLE finance_expenses_next RENAME TO finance_expenses;
ALTER TABLE expense_decisions_next RENAME TO expense_decisions;
CREATE INDEX expense_decisions_claim_idx ON expense_decisions(expense_id,created_at);
CREATE UNIQUE INDEX expense_invoice_idx ON finance_expenses(invoice_id) WHERE invoice_id IS NOT NULL;
CREATE UNIQUE INDEX expense_receipt_idx ON finance_expenses(company_id,employee_id,lower(merchant),lower(receipt_reference))
 WHERE receipt_reference IS NOT NULL AND cancelled_at IS NULL;
CREATE INDEX expense_scope_idx ON finance_expenses(company_id,branch_id,department_id,status);
CREATE INDEX expense_claimant_idx ON finance_expenses(company_id,claimant_user_id,workflow_stage);

ALTER TABLE approval_steps ADD COLUMN approver_pool_json TEXT CHECK(approver_pool_json IS NULL OR json_valid(approver_pool_json));
ALTER TABLE approval_steps ADD COLUMN required_role TEXT;
ALTER TABLE approval_steps ADD COLUMN stage TEXT;
ALTER TABLE approval_steps ADD COLUMN route_snapshot_json TEXT CHECK(route_snapshot_json IS NULL OR json_valid(route_snapshot_json));

CREATE TABLE expense_payment_drafts (
 id TEXT PRIMARY KEY, expense_id TEXT NOT NULL REFERENCES finance_expenses(id), company_id TEXT NOT NULL,
 created_by TEXT NOT NULL, payment_date TEXT, amount_minor INTEGER CHECK(amount_minor IS NULL OR amount_minor>0),
 reference TEXT, version INTEGER NOT NULL DEFAULT 1,
 status TEXT NOT NULL DEFAULT 'draft' CHECK(status IN ('draft','confirmed','cancelled')),
 payment_id TEXT REFERENCES finance_payments(id), created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL
) STRICT;
CREATE INDEX expense_payment_drafts_claim ON expense_payment_drafts(expense_id,status);
