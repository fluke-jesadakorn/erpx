/** r45: byte-identical, parameterized read-only SQL heads shared across files
 * owner adapters. Text only: every call site still prepares and executes its own
 * statement in its supplied database/transaction with its own binds; no SQL text
 * is accepted from a caller and no result is cached here. Advisory locks, row
 * locks, LOCK TABLE, native writes and every r1-r44 helper are left untouched. */

/** Shared unaliased attachment-events witness head (38 bytes). It is deliberately
 * not r38 `... event WHERE event.` nor r40 `... attested WHERE attested...`:
 * neither alias is introduced here. Each site keeps its own WHERE tail, so the
 * rendered statements stay byte-identical to their originals. */
export const ATTACHMENT_EVENTS_WHERE_HEAD_SQL =
  'SELECT 1 FROM attachment_events WHERE ';

/** Shared attachment-sets star projection head (36 bytes). Text only: each site
 * keeps its own predicate/tail; advisoryLockSql, setLockSql FOR UPDATE and
 * nativeWrite statements stay at their call sites. */
export const ATTACHMENT_SETS_STAR_WHERE_HEAD_SQL =
  'SELECT * FROM attachment_sets WHERE ';
