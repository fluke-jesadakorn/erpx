-- Standalone completed-service invoices are discovered from Sales' exact
-- command receipt. Keep that owner lookup bounded on the partitioned receipt
-- ledger without broadening generic Finance invoice source ownership.
CREATE INDEX finance_sales_standalone_invoice_receipt_record
 ON command_receipts(record_id)
 WHERE operation='finance.sales-invoice.prepare';
