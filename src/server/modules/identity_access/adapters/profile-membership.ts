import {identitySnapshotSources as organizationSnapshotSources} from '../../organization/public/identity-snapshot-sources';
const organizationSources=organizationSnapshotSources();
import "server-only";
import type {SqlDatabase} from "../../runtime/public/runtime/types";
export async function activeHoldingProfileMember(write:SqlDatabase,userId:string,now:number){
  return !!await write.prepare(`SELECT 1 FROM assignments a
    ${organizationSources.profile_membership_organization_nodes_c_join} ON c.id=a.company_id AND c.kind='company' AND c.status='active'
    WHERE a.user_id=? AND a.revoked_at IS NULL AND a.valid_from<=? AND a.valid_until>?
      AND iam_assignment_active(a.id,?) AND iam_principal_active(a.user_id,a.company_id,?)
    LIMIT 1`).bind(userId,now,now,now,now).first();
}
