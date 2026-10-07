import type {SqlDatabase} from '../../runtime/public/runtime/types';
import type {PoolClient} from 'pg';

export function betaInvitationSources(db:SqlDatabase,c?:PoolClient){return {async activeBranch(branchId:string,companyId:string){return !!(await c!.query(
            "SELECT 1 FROM organization_nodes WHERE id=$1 AND company_id=$2 AND kind='branch' AND status='active'",
            [branchId, companyId],
          )).rowCount;},
async invitationBranch(branch_id:string){return db
    .prepare(
      "SELECT company_id FROM organization_nodes WHERE id=? AND kind='branch' AND status='active'",
    )
    .bind(branch_id)
    .first<{ company_id: string }>();}};}
