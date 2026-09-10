ALTER TABLE finance_expenses ADD COLUMN reversal_journal_id TEXT REFERENCES finance_journals(id);
UPDATE finance_parties SET kind='employee' WHERE person_id IS NOT NULL AND id LIKE 'expense-payee-%';
