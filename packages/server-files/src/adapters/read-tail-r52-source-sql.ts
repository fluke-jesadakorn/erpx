/** r52: byte-identical read-only audit-event id projection head shared by the
 * files owner denied-read and preview-read audit witnesses. Text only: every
 * call site still prepares and executes its own statement in its supplied
 * database with its own binds; no SQL text is accepted from a caller and no
 * result is cached here. Advisory locks, row locks, LOCK TABLE, native writes
 * and every r1-r51 helper are left untouched. */

/** Shared audit-event id projection head (16 bytes). The denied-read witness
 * completes it with `event.reason,to_jsonb(event)::text witness ...` while the
 * preview-read witness completes it with `to_jsonb(event)::text image ...`;
 * each site keeps its own tail, bind list and ORDER BY, so rendering the head
 * back into either site reproduces the original inline literal byte for byte. */
export const EVENT_ID_PROJECTION_HEAD_SQL = 'SELECT event.id,';
