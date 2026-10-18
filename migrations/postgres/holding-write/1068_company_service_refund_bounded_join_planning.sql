-- The exact 1056 source proof and deferred trigger remain unchanged. Its
-- native 12-relation proof starts from one refund ID and follows original FKs.
-- Bound planning to that declared join order only while this function runs;
-- PostgreSQL restores the caller's prior setting when the function exits.
ALTER FUNCTION public.finance_company_service_refund_source_valid(TEXT)
 SET join_collapse_limit = 1;
