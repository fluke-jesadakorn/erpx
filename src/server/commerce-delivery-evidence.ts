import 'server-only';
import type {WorkContext} from './authorization';
import type {SqlDatabase} from './runtime/types';
import {requirePermission} from './iam-permissions';
import {assertion} from './commands';
import {ApiError} from './http';

type Row=Record<string,string|number|null>;
export async function deliveryEvidenceGuard(db:SqlDatabase,actor:WorkContext,orderId:string,expectedVersion:number,key:string){
 requirePermission(actor,'inventory.prepare');
 const row=await db.prepare("SELECT * FROM commerce_orders WHERE id=? AND company_id=? AND (?::text IS NULL OR branch_id=?) AND (?::text IS NULL OR department_id=?) AND kind='sales' AND state IN ('confirmed','partially_fulfilled')").bind(orderId,actor.companyId,actor.branchId,actor.branchId,actor.departmentId,actor.departmentId).first<Row>();
 if(!row)throw new ApiError(404,'DELIVERY_ORDER_NOT_FOUND','The sales order is unavailable in this work scope or is not ready for delivery.');
 if(Number(row.version)!==expectedVersion)throw new ApiError(409,'VERSION_CONFLICT','The sales order changed. Refresh before attaching delivery evidence.');
 return {row,guards:[assertion(db,key,'delivery-evidence-order',"EXISTS(SELECT 1 FROM commerce_orders WHERE id=? AND company_id=? AND version=? AND kind='sales' AND state IN ('confirmed','partially_fulfilled') AND (?::text IS NULL OR branch_id=?) AND (?::text IS NULL OR department_id=?))",[orderId,actor.companyId,expectedVersion,actor.branchId,actor.branchId,actor.departmentId,actor.departmentId])]};
}
