import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {ORG_ACTIVE_BRANCHES_SOURCE} from './org-sources';
export function betaManagementFacts(db:SqlDatabase){return {async branches(companyId:string,branchId:string | null,branchId2:string | null){return (await db.prepare(
              ORG_ACTIVE_BRANCHES_SOURCE,
            ).bind(companyId, branchId, branchId2)
              .all<{ id: string; display_name: string }>()).results;}};}
