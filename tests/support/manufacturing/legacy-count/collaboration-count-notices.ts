/**
 * FIXTURE-ONLY ORACLE — retained pre-cutover Collaboration inventory-count notices.
 *
 * These two notices were never wired into a live production caller; the only
 * remaining consumer is the isolated legacy-count integration harness. They are
 * kept here as SQL seeding so the legacy assertions can run without a
 * production TS writer in modules/collaboration/server/src.
 *
 * Tracked for removal by `retire.collaboration.residual-writers`: delete once
 * the legacy-count harness is retired or seeds these through native ports.
 */
import type { SqlDatabase } from '@erpx/platform-runtime-server/public/runtime/types';
import { inventoryNotificationAudience } from '@erpx/identity-access-server/public/inventory-authority-projections';

export function inventoryCountSources(db: SqlDatabase) {
  return {
    prepareRemainingHoldNotice(
      value: string,
      company_id: string | number | null,
      remainingHoldId: string,
      now: number,
      company_id2: string | number | null,
      branch_id: string | number | null,
      now2: number,
    ) {
      return db.prepare(`INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at) SELECT ?||'-'||a.user_id,?,a.user_id,'quality_hold',?,1,'Partial scrap posted; remaining returned stock requires a new quality decision',? ${inventoryNotificationAudience.qualityReaders} ON CONFLICT DO NOTHING`)
        .bind(value, company_id, remainingHoldId, now, company_id2, branch_id, now2);
    },
    prepareQualityRequesterNotice(
      value: string,
      companyId: string,
      disposition_requested_by: string | number | null,
      value2: 'quality_hold',
      id: string | number | null,
      value3: number,
      value4: 'Quality scrap posted with inventory and accounting evidence',
      now: number,
    ) {
      return db.prepare('INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at) VALUES(?,?,?,?,?,?,?,?) ON CONFLICT DO NOTHING')
        .bind(value, companyId, disposition_requested_by, value2, id, value3, value4, now);
    },
  };
}
