import 'server-only';
import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';
import { nativeLane24AccessNodes, type Lane24Actor } from './native-lane-24/client';

/**
 * Company-scoped organization node facts. The axum lane-24 owner owns the SQL,
 * the Company bind and the row projection. The caller supplies a WorkContext-
 * derived target (or an explicit registered host/origin); with none, the
 * client fails closed. This adapter never reads ambient request state and
 * prepares no SQL.
 */
export async function peopleAccessNodes(_db: SqlDatabase, companyId: string, target?: Lane24Actor) {
  return nativeLane24AccessNodes(target, companyId);
}
