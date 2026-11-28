WITH previous AS (
 SELECT erpx_service_receipt_runtime_marker() marker
), payment AS (
 SELECT to_jsonb(p) image FROM finance_payments p WHERE p.id=$1 AND p.company_id=$2
), allocations AS (
 SELECT COALESCE(jsonb_agg(to_jsonb(a) ORDER BY a.id),'[]'::jsonb) image FROM finance_allocations a WHERE a.payment_id=$1
), invoices AS (
 SELECT COALESCE(jsonb_object_agg(i.id,to_jsonb(i)),'{}'::jsonb) image FROM finance_invoices i
 WHERE i.company_id=$2 AND EXISTS(SELECT 1 FROM finance_allocations a WHERE a.payment_id=$1 AND a.invoice_id=i.id)
), bank_rows AS (
 SELECT COALESCE(jsonb_object_agg(t.id,to_jsonb(t)),'{}'::jsonb) image FROM finance_bank_transactions t WHERE t.company_id=$2 AND t.id=ANY($3::text[])
), statements AS (
 SELECT COALESCE(jsonb_object_agg(s.id,to_jsonb(s)),'{}'::jsonb) image FROM finance_bank_statements s WHERE s.company_id=$2 AND EXISTS(SELECT 1 FROM finance_bank_transactions t WHERE t.company_id=$2 AND t.statement_id=s.id AND t.id=ANY($3::text[]))
)
SELECT set_config('erpx.service_receipt_runtime',jsonb_build_object(
 'database',current_database(),'pid',pg_backend_pid()::text,'xid',pg_current_xact_id()::text,'started',pg_postmaster_start_time()::text,
 'payments',COALESCE(previous.marker->'payments','{}'::jsonb)||jsonb_build_object($1::text,payment.image),
 'allocations',COALESCE(previous.marker->'allocations','{}'::jsonb)||jsonb_build_object($1::text,allocations.image),
 'invoices',COALESCE(previous.marker->'invoices','{}'::jsonb)||invoices.image,
 'bankRows',COALESCE(previous.marker->'bankRows','{}'::jsonb)||bank_rows.image,
 'bankStatements',COALESCE(previous.marker->'bankStatements','{}'::jsonb)||statements.image
)::text,true) FROM previous,payment,allocations,invoices,bank_rows,statements
