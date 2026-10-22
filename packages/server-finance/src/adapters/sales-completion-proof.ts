type InvoiceAlias = "d" | "i";

/** Exact committed receipt that establishes standalone Sales invoice ownership. */
export function standaloneSalesCompletionReceiptQuery(invoiceAlias: InvoiceAlias) {
  return `SELECT sales_invoice.result_json
    FROM command_receipts sales_invoice
    JOIN audit_events sales_invoice_audit ON sales_invoice_audit.command_id=sales_invoice.command_id
      AND sales_invoice_audit.action='finance.sales-invoice.prepare' AND sales_invoice_audit.actor_id=sales_invoice.actor_id
      AND sales_invoice_audit.record_id=${invoiceAlias}.id AND sales_invoice_audit.record_version=1
      AND sales_invoice_audit.assignment_id=sales_invoice.authority_json::jsonb->>'assignmentId'
    JOIN outbox_events sales_invoice_event ON sales_invoice_event.command_id=sales_invoice.command_id
      AND sales_invoice_event.event_type='finance.sales-invoice.prepare' AND sales_invoice_event.aggregate_id=${invoiceAlias}.id
      AND sales_invoice_event.aggregate_version=1
    WHERE sales_invoice.record_id=${invoiceAlias}.id AND sales_invoice.operation='finance.sales-invoice.prepare'
      AND sales_invoice.authority_json::jsonb->>'companyId'=${invoiceAlias}.company_id
      AND sales_invoice.request_hash ~ '^[a-f0-9]{64}$'
      AND sales_invoice.result_json::jsonb->>'id'=${invoiceAlias}.id
      AND sales_invoice.result_json::jsonb->>'status'='draft' AND sales_invoice.result_json::jsonb->>'version'='1'
      AND sales_invoice.result_json::jsonb->'completionEvidence'->>'confirmedBy'=sales_invoice.actor_id
      AND sales_invoice.result_json::jsonb->'completionEvidence'->>'invoiceRevision'='1'
      AND NULLIF(btrim(sales_invoice.result_json::jsonb->'completionEvidence'->>'note'),'') IS NOT NULL
    ORDER BY sales_invoice.created_at DESC LIMIT 1`;
}
