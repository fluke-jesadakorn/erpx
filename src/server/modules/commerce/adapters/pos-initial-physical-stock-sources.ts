import 'server-only';
import type {SqlDatabase} from '../../runtime/public/runtime/types';
import type {CountRow} from '../../inventory/public/count-contract';
import {initialPhysicalStockCommerceSourceLocks} from './initial-physical-stock-count-sources';
import {posInitialPhysicalStockAccountingRows,posInitialPhysicalStockAccountingLocks} from '../../pos/public/initial-physical-stock-sources';
export {initialPhysicalStockMasterChoices,initialPhysicalStockMasterSource} from './initial-physical-stock-count-sources';
export function posInitialPhysicalStockCommerceLocks(database:SqlDatabase){return [...initialPhysicalStockCommerceSourceLocks(database),...posInitialPhysicalStockAccountingLocks(database)];}
/** Fresh creation source; after creation the retained Inventory receipt lineage
 * stands independently of later legitimate POS settings changes. */
export async function posInitialPhysicalStockCommerceSource(database:SqlDatabase,companyId:string,warehouseId?:string){
 return database.prepare(`SELECT settings.*,mapping.command_id pos_setup_command_id,mapping.request_hash pos_setup_request_hash,accounting.bootstrap_receipt_id,accounting.bootstrap_request_hash,accounting.bootstrap_audit_id,accounting.approval_receipt_id,accounting.approval_request_hash,accounting.approval_audit_id,accounting.accounting_image_hash,accounting.accounting_image,accounting.company_id IS NOT NULL accounting_approved
 FROM commerce_settings settings JOIN command_receipts mapping ON mapping.record_id=settings.company_id AND mapping.operation='commerce.pos-posting-setup.initialize' AND mapping.authority_json::jsonb->>'companyId'=settings.company_id AND mapping.result_json::jsonb->>'id'=settings.company_id AND mapping.result_json::jsonb->>'status'='configured' AND mapping.result_json::jsonb->>'version'='1'
 LEFT JOIN (${posInitialPhysicalStockAccountingRows}) accounting ON accounting.company_id=settings.company_id AND accounting.currency=settings.currency AND accounting.accounts_json::jsonb->>'chartId'=settings.chart_id AND accounting.accounts_json::jsonb->>'inventory'=settings.inventory_account_id AND (?::text IS NULL OR accounting.warehouse_id=?)
 WHERE settings.company_id=? ORDER BY mapping.created_at,mapping.command_id,accounting.bootstrap_receipt_id,accounting.approval_receipt_id LIMIT 1`).bind(warehouseId??null,warehouseId??null,companyId).first<CountRow>();
}
