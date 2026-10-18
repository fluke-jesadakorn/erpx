import 'server-only';
import type {SqlDatabase} from '../../runtime/public/runtime/types';
/** Fixed Organization references for a real HR placement, never automatic
 * creation of a Branch, Level, Position, manager or authority seat. */
export function projectWorkerOrganizationSourceLocks(database:SqlDatabase,writer=false){return [database.prepare('LOCK TABLE organization_nodes,organization_positions,staff_levels,authority_seats IN SHARE MODE'),database.prepare(`LOCK TABLE staff_organization_history,staff_level_history IN ${writer?'SHARE ROW EXCLUSIVE':'SHARE'} MODE`)];}
export async function projectWorkerOrganizationSource(database:SqlDatabase,companyId:string){
 const [nodes,positions,levels,image]=await Promise.all([
  database.prepare("SELECT id,kind,display_name,branch_id FROM organization_nodes WHERE company_id=? AND status='active' AND kind IN ('branch','headquarters','department') ORDER BY kind,id").bind(companyId).all<{id:string;kind:string;display_name:string;branch_id:string|null}>(),
  database.prepare('SELECT id,title,branch_id,department_id,level_id,is_ceo FROM organization_positions WHERE company_id=? AND active=1 ORDER BY title,id').bind(companyId).all<{id:string;title:string;branch_id:string|null;department_id:string|null;level_id:string;is_ceo:boolean|number}>(),
  database.prepare('SELECT id,name FROM staff_levels WHERE company_id=? AND active=1 ORDER BY rank,id').bind(companyId).all<{id:string;name:string}>(),
  database.prepare("SELECT jsonb_build_object('nodes',(SELECT jsonb_agg(to_jsonb(n) ORDER BY n.id) FROM organization_nodes n WHERE n.company_id=?),'positions',(SELECT jsonb_agg(to_jsonb(p) ORDER BY p.id) FROM organization_positions p WHERE p.company_id=?),'levels',(SELECT jsonb_agg(to_jsonb(l) ORDER BY l.id) FROM staff_levels l WHERE l.company_id=?),'seats',(SELECT jsonb_agg(to_jsonb(s) ORDER BY s.kind,s.scope_id) FROM authority_seats s WHERE s.company_id=?))::text image").bind(companyId,companyId,companyId,companyId).first<string>('image'),
 ]);
 return {nodes:nodes.results,positions:positions.results,levels:levels.results,image};
}
