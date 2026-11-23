import 'server-only';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import { nativeCrmActivityWorkspace, nativeCrmActivityWork } from './native-activity-workspace';

/**
 * Retained named exports for the activity workspace/work reads. The five
 * owner SELECTs that lived here (lead, activities, recipients, history and
 * workRows) are gone: the native axum owner now owns them on one serializable
 * unit with the purchase/IAM facts, and the strict client is the only transport.
 * There is no TypeScript SQL fallback; a missing or invalid native reply fails
 * closed.
 */
export function crmActivityWorkspace(a: WorkContext, id: string) {
  return nativeCrmActivityWorkspace(a, id);
}

export function crmActivityWork(a: WorkContext) {
  return nativeCrmActivityWork(a);
}
