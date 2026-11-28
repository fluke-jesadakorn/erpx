import type {SqlDatabase} from '@erpx/platform-runtime-server/public/runtime/types';
import type {PoolClient} from 'pg';
import {ORG_BRANCH_COMPANY_SOURCE} from './org-sources';

export function betaInvitationSources(db:SqlDatabase,c?:PoolClient){return {async activeBranch(branchId:string,companyId:string){return !!(await c!.query(
            "SELECT 1 FROM organization_nodes WHERE id=$1 AND company_id=$2 AND kind='branch' AND status='active'",
            [branchId, companyId],
          )).rowCount;},
async invitationBranch(branch_id:string){return db
    .prepare(
      ORG_BRANCH_COMPANY_SOURCE,
    )
    .bind(branch_id)
    .first<{ company_id: string }>();}};}
