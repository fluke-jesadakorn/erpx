import "server-only";
import { moduleRegistry } from "@/data/module-registry";
import { hasPermission, permissionCatalog } from "@/lib/permissions";
import type { Filters } from "@/lib/executive/canvas";
import type { WorkContext } from "../authorization";
import type { SqlDatabase } from "../runtime/types";
import { currentAuthority } from "./access";
import { metricCatalog, queryMetric } from "./metrics";

/** The model receives an authorized analytical projection, never a database connection or a privileged service identity. */
export async function executiveAiContext(db:SqlDatabase,a:WorkContext,filters:Filters) {
  await currentAuthority(db,a);
  const catalog=metricCatalog(a).filter(m=>m.available);
  const allowedModules=new Set(permissionCatalog.filter(p=>hasPermission(a,p.id)&&(/\.(read|reports)$/.test(p.id)||p.id.startsWith("self."))).map(p=>p.module));
  const modules=moduleRegistry.filter(m=>allowedModules.has(m.id)).map(m=>({id:m.id,name:m.name,description:m.description}));
  const snapshots=[];
  for(const metric of catalog)for(const breakdown of metric.breakdowns??["default"]){
    const data=await queryMetric(db,a,metric.id,filters,breakdown);
    snapshots.push({metric:metric.id,breakdown,total:data.total,unit:data.unit,currency:data.currency,definition:data.definition,scope:data.scope,rows:data.rows.slice(0,20),rowsTruncated:data.rows.length>20,revision:data.revision});
  }
  let organization:unknown={available:false,reason:"The selected role cannot read organization and people data."};
  if(hasPermission(a,"people.read")) {
    const nodes=(await db.prepare(`SELECT id,kind,display_name,parent_id,branch_id FROM organization_nodes WHERE company_id=? AND status='active' AND (kind='company' OR ((?::text IS NULL OR id=? OR branch_id=?) AND (?::text IS NULL OR id=? OR (kind='branch' AND id=(SELECT branch_id FROM organization_nodes WHERE id=? AND company_id=?))))) ORDER BY kind,display_name,id LIMIT 501`).bind(a.companyId,a.branchId,a.branchId,a.branchId,a.departmentId,a.departmentId,a.departmentId,a.companyId).all()).results;
    const positions=(await db.prepare(`SELECT p.id,p.title,p.branch_id,p.department_id,p.level_id,p.reports_to_position_id,l.level_number FROM organization_positions p JOIN staff_levels l ON l.id=p.level_id AND l.company_id=p.company_id WHERE p.company_id=? AND p.active=1 AND l.active=1 AND (?::text IS NULL OR p.branch_id=? OR p.branch_id IS NULL) AND (?::text IS NULL OR p.department_id=? OR p.department_id IS NULL) ORDER BY l.level_number,p.title,p.id LIMIT 501`).bind(a.companyId,a.branchId,a.branchId,a.departmentId,a.departmentId).all()).results;
    const ids=new Set(positions.slice(0,500).map(p=>p.id));
    organization={available:true,nodes:nodes.slice(0,500),positions:positions.slice(0,500).map(p=>({...p,reports_to_position_id:ids.has(p.reports_to_position_id)?p.reports_to_position_id:null})),truncated:nodes.length>500||positions.length>500};
  }
  await currentAuthority(db,a);
  return {scope:{companyId:a.companyId,branchId:a.branchId,departmentId:a.departmentId,role:a.role,roleRevision:a.iam?.roleRevision??null},filters,catalog,modules,organization,snapshots,capturedAt:Date.now(),coverage:"All authorized metric adapters in the selected company and work scope. Module descriptions are structural context, not a claim of access to every underlying record. Never infer missing payroll, profit, occupancy, forecasts or another company's data."};
}
