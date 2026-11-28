import 'server-only';
import type { SqlDatabase } from '@erpx/platform-runtime-server/public/runtime/types';
import {
  sentNoticeDeliveryCount as nativeDeliveryCount,
  sentNoticeRow as nativeNotice,
  sentNoticeRows as nativeNotices,
  taskSocialOrigin as nativeTaskOrigin,
  type Lane16Actor,
} from './native-lane-16/client';

/**
 * People app entrypoint facts. The four reads below are thin transports to the
 * native Collaboration lane; the owner executes the SQL, applies the Company
 * fence and returns the complete projection. No TypeScript fallback remains.
 * A caller with a WorkContext passes it so the native target is selected by its
 * exact (holdingId, holdingSlug); without one the exact incoming Host applies.
 */
export function taskSocialOrigin(db: SqlDatabase, taskId: string, actor?: Lane16Actor) {
  return nativeTaskOrigin(db, taskId, actor);
}
export function sentNoticeRows(db: SqlDatabase, companyId: string, senderId: string, actor?: Lane16Actor) {
  return nativeNotices(db, companyId, senderId, actor);
}
export function sentNoticeRow(db: SqlDatabase, noticeId: string, companyId: string, senderId: string, actor?: Lane16Actor) {
  return nativeNotice(db, noticeId, companyId, senderId, actor);
}
export function sentNoticeDeliveryCount(db: SqlDatabase, companyId: string, eventKey: string, actor?: Lane16Actor) {
  return nativeDeliveryCount(db, companyId, eventKey, actor);
}
