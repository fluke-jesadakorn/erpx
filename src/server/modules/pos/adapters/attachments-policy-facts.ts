import { hospitalityOwnerProjectionSources as commerceProjectionSources } from "../../commerce/public/hospitality-owner-projection-sources";
import { hospitalityOwnerProjectionSources as identity_accessProjectionSources } from "../../identity_access/public/hospitality-owner-projection-sources";
import "server-only";
import type {SqlDatabase} from "../../runtime/public/runtime/types";
export function policyPosFacts(db:SqlDatabase){return {readPosProduct:(facts:{targetId:string;companyId:string;departmentId:string | null;assignmentId:string;userId:string;role:string;now:number;permission:"pos.read" | "pos.configure";branchId:string | null;})=>{return db.prepare(`SELECT product.company_id,product.version FROM (${commerceProjectionSources.commerce_products}) product
    JOIN pos_catalog catalog ON catalog.product_id=product.id AND catalog.company_id=product.company_id
    WHERE product.id=? AND product.company_id=? AND ?::text IS NULL
      AND EXISTS(SELECT 1 FROM (${identity_accessProjectionSources.assignments}) grant_row WHERE grant_row.id=? AND grant_row.user_id=?
        AND grant_row.role=? AND grant_row.company_id=product.company_id
        AND iam_assignment_active(grant_row.id,?) AND iam_principal_active(grant_row.user_id,grant_row.company_id,?)
        AND iam_role_allows(grant_row.role,grant_row.company_id,?)
        AND (grant_row.scope_kind='holding' OR grant_row.scope_kind='branch' AND grant_row.scope_node_id=?))
      AND (?::text IS NULL OR EXISTS(SELECT 1 FROM pos_register_catalog selected
        JOIN pos_registers register_row ON register_row.id=selected.register_id
        WHERE selected.product_id=product.id AND register_row.company_id=product.company_id
          AND register_row.branch_id=? AND selected.enabled=true))`).bind(facts.targetId,facts.companyId,facts.departmentId,facts.assignmentId,facts.userId,facts.role,facts.now,facts.now,facts.permission,facts.branchId,facts.branchId,facts.branchId).first<{ company_id: string; version: number; }>();}};}
