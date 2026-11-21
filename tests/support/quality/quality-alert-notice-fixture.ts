/**
 * FIXTURE-ONLY ORACLE — retained pre-cutover Quality alert notice routing.
 *
 * Production notice routing now belongs to the native quality_alert_worker
 * (`route_quality_alert_notifications` in server-quality). This copy seeds the
 * same notification rows for the integration suite without the TS writer.
 *
 * Tracked for removal by `migrate.quality.alert-notice-worker`: delete once the
 * integration harness seeds notices through the native worker result directly.
 */
import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';

export async function seedQualityAlertNotices(db: SqlDatabase, now: number): Promise<void> {
  await db.prepare(`INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at)
    SELECT DISTINCT a.id||':'||m.user_id,h.company_id,m.user_id,'quality_alert',a.id,a.version,
      CASE WHEN a.phase='overdue' THEN 'Quality hold is overdue: take responsibility for inspection or release follow-up'
        ELSE 'Quality hold is due within 24 hours: prepare the next inspection or release action' END,?::bigint
    FROM inventory_quality_alerts a
    JOIN inventory_quality_holds h ON h.id=a.hold_id
    JOIN assignments m ON m.company_id=h.company_id
      AND iam_assignment_active(m.id,?::double precision::bigint)
      AND iam_role_allows(m.role,m.company_id,CASE WHEN h.status IN ('awaiting_release','cancel_pending') THEN 'quality.release' ELSE 'quality.inspect' END)
    WHERE a.acknowledged_by IS NULL AND h.status NOT IN ('released','cancelled','disposed')
    ON CONFLICT DO NOTHING`).bind(now, now).run();
}
