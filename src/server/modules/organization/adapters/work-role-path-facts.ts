import "server-only";
import type {SqlDatabase} from "../../runtime/public/runtime/types";
export function workRolePathFacts(db:SqlDatabase){return {node:(id:string|null,companyId:string)=>db.prepare('SELECT n.slug,b.slug branch_slug FROM organization_nodes n LEFT JOIN organization_nodes b ON b.id=n.branch_id WHERE n.id=? AND n.company_id=?').bind(id,companyId).first<{slug:string;branch_slug:string|null}>()};}
