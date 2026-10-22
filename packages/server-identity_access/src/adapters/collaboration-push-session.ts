import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';import type {StoredSubscription} from '@erpx/server-collaboration/public/web-push';export async function currentPushSession(control:SqlDatabase,db:SqlDatabase,
  s:Pick<StoredSubscription,"session_id"|"user_id"|"company_id">,now:number) {
  const session=await control.prepare('SELECT 1 FROM session WHERE id=? AND "userId"=? AND "expiresAt">?')
    .bind(s.session_id,s.user_id,new Date(now).toISOString()).first();
  if(!session)return false;
  return Boolean(await db.prepare(`SELECT 1 FROM active_work_contexts c JOIN assignments a
    ON a.id=c.assignment_id AND a.company_id=c.company_id AND a.user_id=c.user_id
    WHERE c.session_id=? AND c.company_id=? AND c.user_id=? AND c.expires_at>?
    AND a.version=c.assignment_version AND a.revoked_at IS NULL
    AND iam_assignment_active(a.id,?) AND iam_principal_active(a.user_id,a.company_id,?)
    AND NOT EXISTS(SELECT 1 FROM iam_revoked_sessions r WHERE r.session_id=c.session_id
      AND r.company_id=c.company_id AND r.user_id=c.user_id) LIMIT 1`)
    .bind(s.session_id,s.company_id,s.user_id,now,now,now).first());
}
