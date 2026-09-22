-- Several approved lists may coexist when their effective dates differ. Resolution selects the latest applicable list.
DROP INDEX sales_price_lists_active_scope;
CREATE INDEX sales_price_lists_active_scope_dates
 ON sales_price_lists(company_id,COALESCE(customer_id,''),valid_from DESC,valid_until,activated_at DESC)
 WHERE status='active';
