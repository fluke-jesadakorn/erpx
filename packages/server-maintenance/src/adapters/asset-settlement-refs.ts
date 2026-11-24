import "server-only";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import {maintenanceSettlementConsumptions} from "@erpx/server-inventory/public/maintenance-settlement-sources";
/** Canonical owner-local Asset settlement reference source shared by the
 * preventive-plan and maintenance-parts refs. Both project a company/asset id
 * pair from an owner-fixed relation; callers pass only closed column and source
 * fragments, never request-authored SQL. Each composed string stays byte-
 * identical to its pre-dedupe literal and no row-locking statement is accepted. */
export const settlementRefSource = (columns: string, source: string): string =>
  `SELECT ${columns} FROM ${source}`;
/** Fixed actual references scope addon withdrawal without selecting Work by
 * the buyer's permissions or by a caller-provided source label. */
export const maintenanceAssetSettlementRefs={
  preventive:settlementRefSource("company_id,asset_id","maintenance_preventive_plans"),
  parts:settlementRefSource("consumption.company_id,work.asset_id",`(${maintenanceSettlementConsumptions}) consumption LEFT JOIN maintenance_work_orders work ON work.id=consumption.source_id AND work.company_id=consumption.company_id`),
} as const;
export function maintenanceAssetSettlementRefReadLocks(database:SqlDatabase){return [database.prepare("LOCK TABLE maintenance_work_orders,maintenance_preventive_plans IN SHARE MODE")];}
