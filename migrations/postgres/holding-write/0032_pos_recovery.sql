-- Recovery metadata never changes the original provider/account binding.
ALTER TABLE pos_oauth_states ADD COLUMN livemode boolean NOT NULL DEFAULT false;
ALTER TABLE pos_sales ADD COLUMN discount_requested_by text;
ALTER TABLE pos_refund_payments ADD COLUMN provider_attempt integer NOT NULL DEFAULT 0;
ALTER TABLE pos_settlements ADD COLUMN version integer NOT NULL DEFAULT 1;
CREATE UNIQUE INDEX pos_payout_bank_match ON pos_settlements(bank_transaction_id) WHERE bank_transaction_id IS NOT NULL;
