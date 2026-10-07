import type {SqlDatabase} from '../../runtime/public/runtime/types';
/** Company scope stays in the caller's guarded statement and uses the supplied database. */
export function aiGatewayCompanySources(db:SqlDatabase){return {
 scopeFence:"EXISTS(SELECT 1 FROM organization_nodes n WHERE n.id=? AND n.kind='company' AND n.company_id=? AND n.status='active')",
 async scope(companyId:string){return !!await db.prepare("SELECT 1 FROM organization_nodes n WHERE n.id=? AND n.kind='company' AND n.company_id=? AND n.status='active'").bind(companyId,companyId).first();},
};}
