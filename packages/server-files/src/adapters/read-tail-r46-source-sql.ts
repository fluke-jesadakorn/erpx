/** r46: closed-union, read-only SQL head families shared by the files read tail.
 * Text only: every call site still prepares and executes its own statement in
 * its supplied database/transaction with its own binds; no result is cached
 * here. Advisory locks, row locks, LOCK TABLE, native writes and every r1-r45
 * helper are left untouched. */

/** Orphan-preservation event witness head. The two reconciliation predicates
 * differ only by the closed event-kind union ('preserved' | 'restored'); each
 * call site keeps its own EXISTS / NOT EXISTS prefix and correlation. Distinct
 * from the preservation-presence helper's complete unaliased statement and
 * from the r45 attachment_events / attachment_sets heads. */
export const ORPHAN_PRESERVATION_EVENT_HEAD_SQL = (kind: 'preserved' | 'restored') =>
  `SELECT 1 FROM attachment_orphan_preservation_events e WHERE e.preservation_id=p.id AND e.event_kind='${kind}'`;

/** Draft/bound link count head. The closed union covers the `AS count` alias
 * used by one witness and omitted by the other; both render byte-identically
 * to their original literals and keep their own binds. */
export const SET_DRAFT_BOUND_COUNT_SQL = (aliased: boolean) =>
  `SELECT count(*) ${aliased ? 'AS count ' : ''}FROM attachment_links l JOIN attachment_sets s ON s.id=l.set_id WHERE l.set_id=? AND s.company_id=? AND l.state IN ('draft','bound')`;
