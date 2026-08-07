-- Catalog plans are projected once in control read, never once per tenant.
DROP TABLE IF EXISTS erpx_commercial_read.catalog_plan_projection;

