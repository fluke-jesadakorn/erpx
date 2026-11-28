import 'server-only';
/** Canonical owner-local projection/source head for the Operations read tail.
 *  Both retired owner renderers opened with these exact bytes; callers append
 *  their own unchanged alias/WHERE tail so rendered SQL, binds/order and locks
 *  stay identical. No locks or viewer-clock state is involved. */
export const OPERATIONS_PROJECTION_FROM_SOURCE = (projection: string, source: string) =>
  `SELECT ${projection} FROM (${source})`;
