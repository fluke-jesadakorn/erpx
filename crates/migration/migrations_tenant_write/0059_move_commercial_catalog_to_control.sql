-- Catalog plans are platform-scoped and authoritative in control write.
-- Tenant databases retain only subscription and entitlement command guards.
DROP TABLE IF EXISTS erpx_commercial_write.catalog_plan_guards;

