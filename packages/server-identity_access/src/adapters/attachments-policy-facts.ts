import "server-only";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
export function policyIdentityAccessFacts(db:SqlDatabase){return {readCurrentAssignment:(facts:{assignmentId:string;companyId:string;userId:string;role:string;now:number;branchId:string | null;departmentId:string | null;})=>{return db.prepare(`SELECT a.role,a.modules_json,
      COALESCE((SELECT v.permissions_json FROM iam_roles r JOIN iam_role_revisions v ON v.company_id=r.company_id AND v.role_id=r.id AND v.revision=r.published_revision
        WHERE r.id=a.role AND r.company_id IN ('*',a.company_id) AND r.status='active' ORDER BY (r.company_id=a.company_id) DESC LIMIT 1),'[]'::jsonb)::text role_permissions,
      COALESCE((SELECT jsonb_agg(p.id) FROM iam_permissions p WHERE iam_role_allows(a.role,a.company_id,p.id)),'[]'::jsonb)::text permissions
    FROM assignments a WHERE a.id=? AND a.company_id=? AND a.user_id=? AND a.role=? AND iam_assignment_active(a.id,?) AND iam_principal_active(a.user_id,a.company_id,?)
      AND (a.scope_kind='holding' OR a.scope_kind='branch' AND a.scope_node_id=? OR a.scope_kind='department' AND a.scope_node_id=?) FOR SHARE OF a`).bind(facts.assignmentId,facts.companyId,facts.userId,facts.role,facts.now,facts.now,facts.branchId,facts.departmentId).first<{ role: string; modules_json: string; role_permissions: string; permissions: string; }>();}};}
