import 'server-only';
import type {WorkContext} from '@erpx/server-identity_access/public/authorization';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';

/** Company foundation facts only; this point witness never grants Organization
 * management or provider access. All native rows remain inside the read unit. */
export function isOrganizationAttachmentReadTarget(target:string){return target==='organization_node'||target==='organization_change';}
export function organizationAttachmentReadSourceLocks(database:SqlDatabase){return [database.prepare('LOCK TABLE organization_nodes,organization_node_changes,organization_node_change_events IN SHARE ROW EXCLUSIVE MODE')];}
export function foundationAttachmentOrganizationSourceLocks(database:SqlDatabase){return [database.prepare('LOCK TABLE organization_nodes IN SHARE ROW EXCLUSIVE MODE')];}
export function foundationAttachmentOrganizationSourceImage(database:SqlDatabase,companyId:string){return database.prepare("SELECT to_jsonb(company)::text image FROM organization_nodes company WHERE company.id=? AND company.kind='company'").bind(companyId).first<string>('image');}
export async function organizationAttachmentReadSourceImage(database:SqlDatabase,actor:WorkContext,type:string,id:string){
 if(!isOrganizationAttachmentReadTarget(type))return null;
 if(type==='organization_node')return database.prepare(`SELECT jsonb_build_object('node',to_jsonb(node),'company',(SELECT to_jsonb(company) FROM organization_nodes company WHERE company.id=node.company_id AND company.kind='company'),
 'branch',(SELECT to_jsonb(branch) FROM organization_nodes branch WHERE branch.id=node.branch_id AND branch.company_id=node.company_id))::text image FROM organization_nodes node WHERE node.id=? AND node.company_id=? AND node.kind IN('company','headquarters','branch','department')`).bind(id,actor.companyId).first<string>('image');
 return database.prepare(`SELECT jsonb_build_object('change',to_jsonb(change),'target',(SELECT to_jsonb(node) FROM organization_nodes node WHERE node.id=change.target_node_id AND node.company_id=change.company_id),
 'branch',(SELECT to_jsonb(branch) FROM organization_nodes branch WHERE branch.id=change.branch_id AND branch.company_id=change.company_id),
 'events',(SELECT jsonb_agg(to_jsonb(event) ORDER BY event.version,event.id) FROM organization_node_change_events event WHERE event.change_id=change.id AND event.company_id=change.company_id))::text image FROM organization_node_changes change WHERE change.id=? AND change.company_id=?`).bind(id,actor.companyId).first<string>('image');
}
