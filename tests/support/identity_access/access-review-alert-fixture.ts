/**
 * FIXTURE-ONLY ORACLE — retained pre-cutover access-certification alert notices.
 *
 * Production due-soon/overdue notices now belong to the native
 * `identity_access_review_alerts` worker (platform/composition). This copy seeds the
 * same notifications for the integration suite without the TS writer.
 *
 * Tracked for removal by `retire.identity_access.residual-writers`: delete once
 * the integration harness drives the native worker result directly.
 */
import type { SqlDatabase } from '@erpx/platform-runtime-server/public/runtime/types';

const HEAD = 'INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at) ';

function recipients(phase: 'due-soon' | 'overdue') {
  const due = phase === 'due-soon' ? 'c.due_at>? AND c.due_at<=?' : 'c.due_at<=?';
  return `SELECT 'iam-access-review-${phase}-'||md5(c.id||':'||c.due_at::text||':'||a.user_id),c.company_id,a.user_id,'iam_access_review',c.id,c.version,?,?
 FROM iam_access_review_campaigns c JOIN assignments a ON a.company_id=c.company_id
 WHERE c.status='open' AND EXISTS(SELECT 1 FROM iam_access_review_items i WHERE i.campaign_id=c.id AND i.decision='pending') AND ${due}
 AND a.scope_kind='holding' AND a.revoked_at IS NULL AND iam_assignment_active(a.id,?) AND iam_role_allows(a.role,a.company_id,'iam.review')
 GROUP BY c.id,c.company_id,c.version,c.due_at,a.user_id`;
}

export async function seedAccessReviewAlerts(
  db: SqlDatabase,
  now: number,
  warningWindowMs = 7 * 86_400_000,
): Promise<{ dueSoon: number; overdue: number }> {
  const dueSoon = await db.prepare(`${HEAD}${recipients('due-soon')} ON CONFLICT DO NOTHING`)
    .bind(
      'Access certification is due within 7 days; review pending assignments or coordinate an independent reviewer',
      now, now, now + warningWindowMs, now,
    ).run();
  const overdue = await db.prepare(`${HEAD}${recipients('overdue')} ON CONFLICT DO NOTHING`)
    .bind('Access certification is overdue; complete the remaining independent decisions', now, now, now)
    .run();
  return {
    dueSoon: Number(dueSoon.meta.changes ?? 0),
    overdue: Number(overdue.meta.changes ?? 0),
  };
}
