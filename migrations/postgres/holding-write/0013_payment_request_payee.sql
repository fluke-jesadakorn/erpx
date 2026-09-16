ALTER TABLE finance_expenses ADD COLUMN payee_party_id TEXT REFERENCES finance_parties(id);
CREATE UNIQUE INDEX finance_payment_slip_once ON finance_payments(payment_slip_id) WHERE payment_slip_id IS NOT NULL;
