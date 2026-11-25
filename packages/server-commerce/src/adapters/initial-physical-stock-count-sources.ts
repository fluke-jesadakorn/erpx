import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import type {WorkContext} from '@erpx/server-identity_access/public/authorization';
import type {CountRow} from '@erpx/server-inventory/public/count-contract';
import { nativeCommerceReadAll, nativeCommerceReadFirst, RD_COUNT_ACTIVE_STOCK_PRODUCTS, RD_WAREHOUSE_ACTIVE, RD_INITIAL_COUNT_WAREHOUSES, RD_INITIAL_COUNT_STOCK_PRODUCT } from './native-commerce-read';
/** Private current Commerce reference facts for the exact Inventory initial
 * physical count. No catalogue/Finance collection or caller SQL is accepted. */
export const initialPhysicalStockGoodsReceiptRows=`SELECT s.*,r.command_id goods_setup_command_id,r.request_hash goods_setup_request_hash FROM commerce_settings s JOIN command_receipts r ON r.record_id=s.company_id AND r.operation='commerce.sales-goods-posting-setup.initialize' AND r.authority_json::jsonb->>'companyId'=s.company_id AND r.result_json::jsonb->>'id'=s.company_id AND r.result_json::jsonb->>'status'='configured' AND r.result_json::jsonb->>'version'='1'`;
export async function initialPhysicalStockCommerceSource(database:SqlDatabase,companyId:string){
 return database.prepare(`SELECT s.*,r.command_id goods_setup_command_id,r.request_hash goods_setup_request_hash
  FROM commerce_settings s JOIN command_receipts r ON r.record_id=s.company_id
   AND r.operation='commerce.sales-goods-posting-setup.initialize'
   AND r.authority_json::jsonb->>'companyId'=s.company_id
   AND r.result_json::jsonb->>'id'=s.company_id AND r.result_json::jsonb->>'status'='configured'
   AND r.result_json::jsonb->>'version'='1'
  WHERE s.company_id=? ORDER BY r.created_at,r.command_id LIMIT 1`).bind(companyId).first<CountRow>();
}
export async function initialPhysicalStockMasterChoices(database:SqlDatabase,actor:Pick<WorkContext,'companyId'|'branchId'>){
 const [warehouses,products]=await Promise.all([
  nativeCommerceReadAll<CountRow>(RD_INITIAL_COUNT_WAREHOUSES,[actor.companyId,actor.branchId,actor.branchId],database),
  nativeCommerceReadAll<CountRow>(RD_COUNT_ACTIVE_STOCK_PRODUCTS,[actor.companyId],database),
 ]);return {warehouses,products};
}
export async function initialPhysicalStockMasterSource(database:SqlDatabase,actor:Pick<WorkContext,'companyId'|'branchId'>,warehouseId:string,productId:string){
 const [warehouse,product]=await Promise.all([
  nativeCommerceReadFirst<CountRow>(RD_WAREHOUSE_ACTIVE,[warehouseId,actor.companyId,actor.branchId,actor.branchId],database),
  nativeCommerceReadFirst<CountRow>(RD_INITIAL_COUNT_STOCK_PRODUCT,[productId,actor.companyId],database),
 ]);return warehouse&&product?{warehouse,product}:null;
}
export function initialPhysicalStockCommerceSourceLocks(database:SqlDatabase){return [database.prepare('LOCK TABLE commerce_stock IN SHARE ROW EXCLUSIVE MODE'),database.prepare('LOCK TABLE commerce_settings,commerce_warehouses,commerce_products IN SHARE MODE')];}
