/** r50: native-owned DISTINCT set-row projection head. The statement bytes live
 * in the Rust-owned contract `projection-head-sources.json`; the leave-evidence
 * canonical projection and the generic set-row draft witness each keep their
 * own tail, binds and transaction. Advisory locks, row locks, LOCK TABLE,
 * native writes and every r1-r49 helper are left untouched. */
import contract from "./projection-head-sources.json" with { type: "json" };

export const DISTINCT_SET_ROW_HEAD_SQL =
  (contract as { readonly distinctSetRow: string }).distinctSetRow;
