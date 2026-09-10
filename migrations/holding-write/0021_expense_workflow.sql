ALTER TABLE finance_expenses ADD COLUMN branch_id TEXT REFERENCES organization_nodes(id);
ALTER TABLE finance_expenses ADD COLUMN department_id TEXT REFERENCES organization_nodes(id);
ALTER TABLE finance_expenses ADD COLUMN engagement_id TEXT REFERENCES hr_engagements(id);
ALTER TABLE finance_expenses ADD COLUMN business_purpose TEXT NOT NULL DEFAULT '';
ALTER TABLE finance_expenses ADD COLUMN receipt_reference TEXT;
ALTER TABLE finance_expenses ADD COLUMN rejection_reason TEXT;
ALTER TABLE finance_expenses ADD COLUMN prepared_by TEXT;
ALTER TABLE finance_expenses ADD COLUMN submitted_at INTEGER;
ALTER TABLE finance_expenses ADD COLUMN posted_at INTEGER;
ALTER TABLE finance_expenses ADD COLUMN reimbursed_at INTEGER;
ALTER TABLE finance_expenses ADD COLUMN cancelled_at INTEGER;
ALTER TABLE finance_expenses ADD COLUMN invoice_id TEXT REFERENCES finance_invoices(id);
CREATE UNIQUE INDEX expense_invoice_idx ON finance_expenses(invoice_id) WHERE invoice_id IS NOT NULL;
CREATE UNIQUE INDEX expense_receipt_idx ON finance_expenses(company_id,employee_id,lower(merchant),lower(receipt_reference))
  WHERE receipt_reference IS NOT NULL AND cancelled_at IS NULL;
CREATE INDEX expense_scope_idx ON finance_expenses(company_id,branch_id,department_id,status);
ALTER TABLE finance_parties ADD COLUMN person_id TEXT REFERENCES hr_people(id);
CREATE UNIQUE INDEX expense_payee_idx ON finance_parties(company_id,person_id) WHERE person_id IS NOT NULL;

UPDATE finance_expenses SET
  engagement_id=(SELECT id FROM hr_engagements e WHERE e.person_id=finance_expenses.employee_id AND e.company_id=finance_expenses.company_id ORDER BY start_date DESC LIMIT 1),
  prepared_by=created_by;
UPDATE finance_expenses SET
  branch_id=(SELECT branch_id FROM hr_engagements e WHERE e.id=finance_expenses.engagement_id),
  department_id=(SELECT department_id FROM hr_engagements e WHERE e.id=finance_expenses.engagement_id);
