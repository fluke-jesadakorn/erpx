/** r53: native-owned attachment source-state projection head. The statement
 * bytes live in the Rust-owned contract `projection-head-sources.json`; the two
 * source-state witnesses keep their own tail and bind. Advisory locks, row
 * locks, LOCK TABLE, native writes and every r1-r52 helper are left untouched. */
import contract from "./projection-head-sources.json" with { type: "json" };

export const SOURCE_STATE_HEAD_SQL =
  (contract as { readonly sourceState: string }).sourceState;
