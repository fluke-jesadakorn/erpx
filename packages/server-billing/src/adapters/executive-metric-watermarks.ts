import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';
import {
  nativeBillingWatermarks,
  type NativeLane38Context,
  type NativeLane38Watermark,
} from './native-lane-38/client';

/**
 * Native lane 38 cutover. The statement and revision projection now live in the
 * Billing owner crate; this adapter is a strict, fail-closed transport. The
 * `sourceDb` argument is retained for call-site compatibility only: no query is
 * issued on it and there is no TypeScript fallback. Callers that hold a full
 * WorkContext should pass `{holdingId, holdingSlug, companyId?}` so the native
 * target is selected from both identity fields; the retained executive caller
 * still supplies a holding id, which must resolve to exactly one target.
 */
export async function executiveBillingWatermarks(
  _sourceDb: SqlDatabase,
  context: string | NativeLane38Context,
): Promise<NativeLane38Watermark[]> {
  return nativeBillingWatermarks(context);
}
