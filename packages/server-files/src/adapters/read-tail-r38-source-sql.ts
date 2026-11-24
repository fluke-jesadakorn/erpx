/** r38: byte-identical, parameterized read-only EXISTS head shared by the
 * compatibility-set witness (app sources), the set.created candidate witness
 * (service repository) and the immutable withdrawal witness (messaging).
 * Text only: every call site keeps its trailing predicate, binds and
 * transaction and renders the exact original statement byte for byte; no SQL
 * text is accepted from a caller and no result is cached here. Advisory locks,
 * row locks, native writes and every r1-r37 helper are left untouched. */

/** Common read-only attachment_events EXISTS projection. The trailing alias
 * predicate stays at each call site, so every rendered statement is unchanged. */
export const ATTACHMENT_EVENT_EVENT_HEAD_SQL =
  'SELECT 1 FROM attachment_events event WHERE event.';
