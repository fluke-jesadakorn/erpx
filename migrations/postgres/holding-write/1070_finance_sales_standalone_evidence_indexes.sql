-- Bound the exact command-identity joins used to prove standalone Sales
-- invoice ownership. These immutable event ledgers otherwise have no
-- command_id index.
CREATE INDEX finance_sales_standalone_invoice_audit_command
 ON audit_events(command_id)
 WHERE action='finance.sales-invoice.prepare';

CREATE INDEX finance_sales_standalone_invoice_outbox_command
 ON outbox_events(command_id)
 WHERE event_type='finance.sales-invoice.prepare';
