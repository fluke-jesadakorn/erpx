import 'server-only';
import type {SqlDatabase} from '../../runtime/public/runtime/types';
import type {WorkContext} from '../../identity_access/public/authorization';
import {hasPermission} from '@/lib/identity_access/permissions';
import {hasModuleAccess} from '@/data/module-access';
/** Exact native physical links and original retirement/Maintenance sources.
 * No physical record is created or relabelled by the Finance continuation. */
export function assetsFinancialPhysicalSourceLocks(database:SqlDatabase){return [database.prepare('LOCK TABLE operational_assets,operational_asset_events,operational_asset_finance_links,maintenance_work_orders IN SHARE MODE')];}
export async function assetsFinancialPhysicalSource(database:SqlDatabase,companyId:string,financeAssetId:string){
 const value=await database.prepare(`WITH physical AS MATERIALIZED(SELECT a.* FROM operational_assets a WHERE a.company_id=? AND a.finance_asset_id=?)
 SELECT jsonb_build_object('assets',(SELECT jsonb_agg(to_jsonb(a) ORDER BY a.id) FROM physical a),'events',(SELECT jsonb_agg(to_jsonb(e) ORDER BY e.id) FROM operational_asset_events e JOIN physical a ON a.id=e.asset_id),'links',(SELECT jsonb_agg(to_jsonb(l) ORDER BY l.id) FROM operational_asset_finance_links l WHERE l.finance_asset_id=?),'work',(SELECT jsonb_agg(to_jsonb(w) ORDER BY w.id) FROM maintenance_work_orders w JOIN physical a ON a.id=w.asset_id))::text image,
 EXISTS(SELECT 1 FROM operational_asset_finance_links l LEFT JOIN physical a ON a.id=l.asset_id WHERE l.finance_asset_id=? AND a.id IS NULL)
 OR EXISTS(SELECT 1 FROM maintenance_work_orders w JOIN physical a ON a.id=w.asset_id WHERE w.company_id<>a.company_id) invalid`).bind(companyId,financeAssetId,financeAssetId,financeAssetId).first<{image:string;invalid:boolean}>();
 if(!value)throw Error('Original physical Asset source unavailable');return value;
}
export async function assetsFinancialPhysicalLinks(database:SqlDatabase,actor:WorkContext,financeAssetId:string){
 if(!hasPermission(actor,'records.read')||!hasModuleAccess(actor.modules,'assets'))return [];
 return (await database.prepare('SELECT id,code,name,lifecycle_status status,version FROM operational_assets WHERE company_id=? AND finance_asset_id=? AND (?::text IS NULL OR branch_id=?) AND (?::text IS NULL OR department_id=?) ORDER BY id').bind(actor.companyId,financeAssetId,actor.branchId,actor.branchId,actor.departmentId,actor.departmentId).all<{id:string;code:string;name:string;status:string;version:number}>()).results;
}
