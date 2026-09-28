-- One provider statement can contain separately denominated currency totals.
-- Keep each currency's reconciliation independently verifiable and immutable.
ALTER TABLE billing_target_statements DROP CONSTRAINT billing_target_statements_pkey;
ALTER TABLE billing_target_statements
  ADD PRIMARY KEY (holding_id,month,provider,statement_id,original_currency);
