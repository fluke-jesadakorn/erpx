import "server-only";
import {assetOperationalRowsSql} from "./asset-operational-rows-sql";
/** Closed native projections for Legal, Asset, Maintenance and Customer joins. Callers retain their original scope/authority filters and execute the composed statement in their supplied transaction. No SQL or relation name is accepted from a caller. */
export const finalRootOwnerProjectionSources = {
  operational_assets: assetOperationalRowsSql("\"id\",\"company_id\",\"branch_id\",\"department_id\",\"code\",\"name\",\"category\",\"currency\",\"cost_minor\",\"condition\",\"lifecycle_status\",\"placement\",\"custodian_id\",\"acquired_date\",\"warranty_end\",\"retirement_reason\",\"version\",\"created_by\",\"created_at\",\"updated_at\",\"finance_asset_id\",\"finance_asset_version\""),
} as const;
