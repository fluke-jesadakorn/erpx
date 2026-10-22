import { finalRootOwnerProjectionSources as organizationProjectionSources } from "@erpx/server-organization/public/final-root-owner-projection-sources";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
export function companyAcceptanceFacts(db:SqlDatabase){return {
async levels(companyId:string){return db.prepare(`SELECT count(*) count FROM (${organizationProjectionSources.staff_levels}) staff_levels WHERE company_id=? AND active=1 AND level_number BETWEEN 1 AND 6`).bind(companyId).first<{ count: number }>();}
};}
