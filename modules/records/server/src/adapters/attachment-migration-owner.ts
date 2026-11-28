import 'server-only';
import type { SqlDatabase } from '@erpx/platform-runtime-server/public/runtime/types';
import {
  readNativeAttachmentOwnerScope,
  type HoldingIdentity,
} from './native-lane-35/multi-holding-client';

/**
 * Wave 2 cutover. The owner statement, descriptor admission, scope projection
 * and response shape live in the Rust axum/sqlx route. This adapter is a thin
 * multi-holding transport only and fails closed when the native reader is down.
 *
 * The Files consumer threads its WorkContext `(holdingId, holdingSlug)` here.
 * The identity stays optional only for adapters that cannot supply one; without
 * it the native read fails closed rather than guessing a Holding.
 */
export async function legacyAttachmentScoperecords(
  _db: SqlDatabase,
  kind: string,
  id: string,
  context?: HoldingIdentity,
  origin?: string,
): Promise<{
  company_id: string;
  branch_id: string | null;
  department_id: string | null;
  version: number;
} | null> {
  const scope = await readNativeAttachmentOwnerScope(kind, id, context, origin);
  return scope
    ? {
        company_id: scope.companyId,
        branch_id: scope.branchId,
        department_id: scope.departmentId,
        version: scope.version,
      }
    : null;
}
