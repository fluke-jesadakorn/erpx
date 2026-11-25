/** Byte-identical parameterized bound-row image aggregate head shared by the
 * Support ticket-portal binding image reader and its bound-row image reader.
 * Only the projected row alias, its ordering expression and the trailing FROM
 * body vary; this renderer owns the single shared read token and emits text
 * only. Binds, locks, native read kinds and execution stay at the call sites. */
export const readTailR44ImageAggregateSql = (target: string, order: string, body: string) =>
  `(SELECT jsonb_agg(to_jsonb(${target}) ORDER BY ${order}) FROM ${body})`;
