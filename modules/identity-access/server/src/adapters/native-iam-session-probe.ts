import 'server-only';
import { ApiError } from '@erpx/platform-runtime-server/public/http';
import {
  nativeSessionList,
  type NativeSessionRow,
} from '@erpx/identity-access-server/public/native-session-list';

const id = (value: unknown): value is string =>
  typeof value === 'string' &&
  value.length > 0 &&
  value.length <= 200 &&
  /^[A-Za-z0-9_.:-]+$/.test(value);

/**
 * Owner-side cutover of the account IAM catch-all `sessions` probe.
 *
 * The mounted native lane 45 `session-list-read` is the only authority: it
 * executes the former `sessionByUserSql` SELECT against PostgreSQL and returns
 * the same `{ id, createdAt, expiresAt, userAgent }` rows. The authenticated
 * Account user is injected as `x-erpx-context-user` and the registered Host
 * selects the Holding target. No SQL, no TypeScript fallback: missing native
 * configuration, an unregistered target or a failed read throws.
 */
export async function nativeIamSessionProbe(
  actor: Readonly<{ userId: string; companyId?: string }> | undefined,
  incoming: Pick<Headers, 'get'>,
): Promise<NativeSessionRow[]> {
  if (
    !actor ||
    !id(actor.userId) ||
    (actor.companyId !== undefined && !id(actor.companyId))
  )
    throw new ApiError(
      422,
      'NATIVE_IAM_SESSION_PROBE_REQUEST_INVALID',
      'Use a bounded IAM actor.',
    );
  return nativeSessionList(
    {
      userId: actor.userId,
      holdingId: null,
      ...(actor.companyId === undefined ? {} : { companyId: actor.companyId }),
    },
    incoming,
  );
}
