import "server-only";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import {maintenanceSettlementConsumptions} from "@erpx/server-inventory/public/maintenance-settlement-sources";
/** Fixed actual references scope addon withdrawal without selecting Work by
 * the buyer's permissions or by a caller-provided source label. */
export const maintenanceAssetSettlementRefs={preventive:"SELECT company_id,asset_id FROM maintenance_preventive_plans",parts:`SELECT consumption.company_id,work.asset_id FROM (${maintenanceSettlementConsumptions}) consumption LEFT JOIN maintenance_work_orders work ON work.id=consumption.source_id AND work.company_id=consumption.company_id`} as const;
export function maintenanceAssetSettlementRefReadLocks(database:SqlDatabase){return [database.prepare("LOCK TABLE maintenance_work_orders,maintenance_preventive_plans IN SHARE MODE")];}
