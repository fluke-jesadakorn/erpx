import "server-only";
import { z } from "zod";
import type { WorkContext } from "../authorization";
import type { SqlDatabase } from "../runtime/types";
import { requirePermission } from "../iam-permissions";
import { access, fail } from "./access";

export async function legalPeople(db:SqlDatabase,a:WorkContext,raw:unknown={}){
  requirePermission(a,"legal.read");
  const q=z.object({purpose:z.enum(["owner","reviewer","member","signer"]).default("owner"),itemId:z.string().optional(),search:z.string().trim().max(100).default(""),role:z.string().max(100).optional(),branchId:z.string().optional(),departmentId:z.string().optional()}).parse(raw);
  const item=q.itemId?await access(db,a,q.itemId):null;
  const branch=item?item.branch_id:q.branchId??a.branchId;
  const department=item?item.department_id:q.departmentId??a.departmentId;
  if((a.branchId&&branch!==a.branchId)||(a.departmentId&&department!==a.departmentId))fail("Choose people in your current organization scope.","LEGAL_SCOPE",403);
  for(const [id,kind] of [[q.branchId,"branch"],[q.departmentId,"department"]])if(id&&!(await db.prepare("SELECT 1 FROM organization_nodes WHERE id=? AND company_id=? AND kind=? AND status='active'").bind(id,a.companyId,kind).first()))fail("Choose an active unit in this Company.","LEGAL_SCOPE",403);
  if(department&&branch&&!(await db.prepare("SELECT 1 FROM organization_nodes WHERE id=? AND company_id=? AND branch_id=? AND status='active'").bind(department,a.companyId,branch).first()))fail("Choose a Department in this Branch.","LEGAL_SCOPE",403);
  const permission=q.purpose==="reviewer"?"legal.approve":null;
  const rows=(await db.prepare(`SELECT DISTINCT a.user_id id,coalesce(p.legal_name,a.user_id) name,a.role
    FROM assignments a LEFT JOIN hr_people p ON p.user_id=a.user_id AND p.company_id=a.company_id AND p.merged_into_id IS NULL
    WHERE a.company_id=? AND legal_person_eligible(a.company_id,a.user_id,?,?,?)
    AND iam_assignment_active(a.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)
    AND (a.scope_kind='holding' OR a.scope_kind='branch' AND a.scope_node_id=? OR a.scope_kind='department' AND a.scope_node_id=?)
    AND (?::text IS NULL OR iam_role_allows(a.role,a.company_id,?))
    AND (?::text IS NULL OR a.role=?)
    AND (?='' OR strpos(lower(coalesce(p.legal_name,a.user_id)),lower(?))>0)
    AND (?::text IS NULL OR ?!='reviewer' OR EXISTS(SELECT 1 FROM legal_members m WHERE m.item_id=? AND m.user_id=a.user_id))
    ORDER BY name,id,a.role LIMIT 100`).bind(a.companyId,branch,department,permission,branch,department,permission,permission,q.role??null,q.role??null,q.search,q.search,item?.id??null,q.purpose,item?.id??null).all<{id:string;name:string;role:string}>()).results;
  return [...new Map(rows.map(p=>[p.id,p])).values()].map(p=>p.id===a.userId?{...p,name:a.name}:p);
}
