import type {SqlDatabase} from '../../runtime/public/runtime/types';
import type {WorkContext} from '../../identity_access/public/authorization';
import type {CountRow} from '../../inventory/public/count-contract';
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
  database.prepare("SELECT id,name,branch_id FROM commerce_warehouses WHERE company_id=? AND status='active' AND (?::text IS NULL OR branch_id=?) ORDER BY name,id").bind(actor.companyId,actor.branchId,actor.branchId).all<CountRow>(),
  database.prepare("SELECT id,sku,name,unit,tracking_mode FROM commerce_products WHERE company_id=? AND status='active' AND item_type='stock' ORDER BY sku,id").bind(actor.companyId).all<CountRow>(),
 ]);return {warehouses:warehouses.results,products:products.results};
}
export async function initialPhysicalStockMasterSource(database:SqlDatabase,actor:Pick<WorkContext,'companyId'|'branchId'>,warehouseId:string,productId:string){
 const [warehouse,product]=await Promise.all([
  database.prepare("SELECT * FROM commerce_warehouses WHERE id=? AND company_id=? AND status='active' AND (?::text IS NULL OR branch_id=?)").bind(warehouseId,actor.companyId,actor.branchId,actor.branchId).first<CountRow>(),
  database.prepare("SELECT * FROM commerce_products WHERE id=? AND company_id=? AND status='active' AND item_type='stock'").bind(productId,actor.companyId).first<CountRow>(),
 ]);return warehouse&&product?{warehouse,product}:null;
}
export function initialPhysicalStockCommerceSourceLocks(database:SqlDatabase){return [database.prepare('LOCK TABLE commerce_stock IN SHARE ROW EXCLUSIVE MODE'),database.prepare('LOCK TABLE commerce_settings,commerce_warehouses,commerce_products IN SHARE MODE')];}
