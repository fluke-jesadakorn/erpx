import 'server-only';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
/** Actual initial setup state, under the existing complete Commerce source
 * leases. It authorizes neither a package nor a Finance command. */
export async function commercePostingPrerequisiteState(database:SqlDatabase,companyId:string){
 const row=await database.prepare(`SELECT EXISTS(SELECT 1 FROM commerce_settings WHERE company_id=?) configured,
  EXISTS(SELECT 1 FROM commerce_orders WHERE company_id=?)
  OR EXISTS(SELECT 1 FROM commerce_stock_moves m JOIN commerce_warehouses w ON w.id=m.warehouse_id WHERE w.company_id=?)
  OR EXISTS(SELECT 1 FROM commerce_stock s JOIN commerce_warehouses w ON w.id=s.warehouse_id WHERE w.company_id=? AND (s.quantity_milli<>0 OR s.value_minor<>0))
  OR EXISTS(SELECT 1 FROM commerce_goods_valuations WHERE company_id=?) history`).bind(companyId,companyId,companyId,companyId,companyId).first<{configured:boolean;history:boolean}>();
 if(!row)throw Error('Commerce initial posting state is unavailable.');return row;
}
