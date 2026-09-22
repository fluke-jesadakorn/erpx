-- Preserve the latest price-list preparer so activation remains independently reviewed after draft edits.
ALTER TABLE sales_price_lists ADD COLUMN prepared_by TEXT;
ALTER TABLE sales_price_lists DISABLE TRIGGER sales_price_list_guard;
UPDATE sales_price_lists SET prepared_by=created_by WHERE prepared_by IS NULL;
ALTER TABLE sales_price_lists ENABLE TRIGGER sales_price_list_guard;
ALTER TABLE sales_price_lists ALTER COLUMN prepared_by SET NOT NULL;
ALTER TABLE sales_price_lists ADD CONSTRAINT sales_price_lists_independent_latest_preparer
 CHECK(status<>'active' OR approved_by<>prepared_by);
