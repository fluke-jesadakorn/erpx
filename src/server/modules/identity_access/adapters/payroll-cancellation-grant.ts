import {identitySnapshotSources as organizationSnapshotSources} from '../../organization/public/identity-snapshot-sources';
const organizationSources=organizationSnapshotSources();
import 'server-only';import type {SqlDatabase,SqlStatement} from '../../runtime/public/runtime/types';import type {WorkContext} from '../public/authorization';import {assertion} from '../../workflow/public/commands';const currentGrant = `EXISTS(SELECT 1 FROM assignments a
  WHERE a.id=? AND a.company_id=? AND a.user_id=? AND a.role=? AND a.scope_kind='holding'
  AND iam_assignment_active(a.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)
  AND iam_principal_active(a.user_id,a.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)
  AND iam_role_allows(a.role,a.company_id,'payroll.prepare')
  AND EXISTS(SELECT 1 ${organizationSources.payroll_cancellation_grant_organization_nodes_c_from} WHERE c.id=a.company_id AND c.status='active'))`;
export async function payrollCancellationGrant(db: SqlDatabase, actor: WorkContext) {
  const grant = await db.prepare(`SELECT 1 WHERE ${currentGrant}`)
    .bind(actor.assignmentId, actor.companyId, actor.userId, actor.role).first();
  return !!grant;
}
export function payrollCancellationGrantLock(db:SqlDatabase,actor:WorkContext):SqlStatement{return db.prepare('SELECT id FROM assignments WHERE id=? FOR SHARE').bind(actor.assignmentId);}
export function payrollCancellationGrantGuard(db:SqlDatabase,key:string,actor:WorkContext):SqlStatement{return assertion(db,key,'payroll-cancellation-current-grant',currentGrant,[actor.assignmentId,actor.companyId,actor.userId,actor.role]);}
