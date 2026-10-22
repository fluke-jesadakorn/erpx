import "server-only";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
export function executiveConnectionOrganizationFacts(db:SqlDatabase){return {organizationScopes:(facts:{value1:string;value2:string | null;value3:string | null;value4:string | null;value5:string | null;value6:string | null;})=>db.prepare(`SELECT n.id,n.kind,n.display_name,n.branch_id,b.display_name branch_name
    FROM organization_nodes n LEFT JOIN organization_nodes b ON b.id=n.branch_id AND b.company_id=n.company_id AND b.status='active'
    WHERE n.company_id=? AND n.status='active' AND n.kind IN ('company','branch','department')
      AND (n.branch_id IS NULL OR b.id IS NOT NULL)
      AND (?::text IS NULL OR n.id=? OR n.branch_id=?)
      AND (?::text IS NULL OR n.id=?) ORDER BY n.kind,n.display_name,n.id`).bind(facts.value1,facts.value2,facts.value3,facts.value4,facts.value5,facts.value6).all<{id:string;kind:string;display_name:string;branch_id:string|null;branch_name:string|null}>()};}
