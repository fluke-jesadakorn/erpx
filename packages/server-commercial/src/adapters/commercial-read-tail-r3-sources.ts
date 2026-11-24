import "server-only";
/** Parameterized SELECT-only family shared by Commercial seat-history readers
 * (G4 read-tail r3). The builder owns one SQL literal; each consumer still
 * prepares and binds its own independent statement, so no read is removed and
 * no export changes. Arguments are code-owned column/table fragments only,
 * never request input. Pure read: no INSERT, UPDATE, DELETE, LOCK TABLE,
 * FOR UPDATE/SHARE or advisory lock. */
export function seatHistorySql(key: string, kind: string, order: string) {
  return `SELECT event_id,${key},captured_at,present,state::text state_json FROM billing_seat_${kind}_events WHERE captured_at<? ORDER BY ${order},captured_at,event_id`;
}
