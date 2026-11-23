export { getSupportIncident, supportIncidentReadPredicate } from '@erpx/server-support/adapters/support-incident-read';

import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import { ApiError } from '@erpx/server-runtime/public/http';

/**
 * Bounded by-id Support incident read client. Native read ports return exactly
 * these five fields; every other database fact stays behind the owner boundary.
 * Consumers switch to this surface after their native route mounts; the legacy
 * `getSupportIncident` re-export above remains until those switches land.
 */
export type SupportIncidentRead = {
  id: string;
  version: number;
  status: string;
  title: string;
  href: string;
};

export const SUPPORT_INCIDENT_READ_STATUSES = new Set([
  'open',
  'investigating',
  'mitigated',
  'closed',
  'cancelled',
]);

const record = (value: unknown): value is Record<string, unknown> =>
  !!value && typeof value === 'object' && !Array.isArray(value);

const fail = (): never => {
  throw new ApiError(
    503,
    'NATIVE_SUPPORT_INCIDENT_READ_INVALID',
    'The native Support incident reader returned an invalid projection.',
  );
};

/** Strictly project one native row to the bounded read contract. */
export function projectSupportIncidentRead(value: unknown): SupportIncidentRead {
  if (!record(value) || Object.keys(value).length !== 5) return fail();
  const { id, version, status, title, href } = value;
  if (
    typeof id !== 'string' ||
    id.length < 1 ||
    id.length > 200 ||
    !/^[A-Za-z0-9_-]+$/.test(id) ||
    typeof version !== 'number' ||
    !Number.isSafeInteger(version) ||
    version < 1 ||
    typeof status !== 'string' ||
    !SUPPORT_INCIDENT_READ_STATUSES.has(status) ||
    typeof title !== 'string' ||
    title.length < 1 ||
    title.length > 160 ||
    href !== `/support/incidents/${id}`
  ) {
    return fail();
  }
  return { id, version, status, title, href };
}

/** Native by-id read port supplied by the mounted owner route. */
export type SupportIncidentReadPort = (
  actor: WorkContext,
  id: string,
) => Promise<unknown>;

/**
 * Build a bounded client over a native read port. The id is validated before
 * dispatch and each result is projected to exactly the five consumer fields.
 */
export function createSupportIncidentReadClient(read: SupportIncidentReadPort) {
  return async (actor: WorkContext, id: string): Promise<SupportIncidentRead> => {
    if (
      typeof id !== 'string' ||
      id.length < 1 ||
      id.length > 200 ||
      !/^[A-Za-z0-9_-]+$/.test(id)
    ) {
      throw new ApiError(422, 'SUPPORT_INCIDENT_ID_INVALID', 'Provide a valid incident id.');
    }
    return projectSupportIncidentRead(await read(actor, id));
  };
}
