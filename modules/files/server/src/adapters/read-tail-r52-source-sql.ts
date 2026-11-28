/** r52: native-owned audit-event id projection head. The statement bytes live
 * in the Rust-owned contract `projection-head-sources.json`; the denied-read and
 * preview-read witnesses each keep their own tail, bind list and ORDER BY.
 * Advisory locks, row locks, LOCK TABLE, native writes and every r1-r51 helper
 * are left untouched. */
import contract from "./projection-head-sources.json" with { type: "json" };

export const EVENT_ID_PROJECTION_HEAD_SQL =
  (contract as { readonly eventId: string }).eventId;
