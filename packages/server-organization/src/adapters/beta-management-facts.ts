import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
export function betaManagementFacts(db:SqlDatabase){return {async branches(companyId:string,branchId:string | null,branchId2:string | null){return (await db.prepare(
              "SELECT id,display_name FROM organization_nodes WHERE kind='branch' AND company_id=? AND status='active' AND (?::text IS NULL OR id=?)",
            ).bind(companyId, branchId, branchId2)
              .all<{ id: string; display_name: string }>()).results;}};}
