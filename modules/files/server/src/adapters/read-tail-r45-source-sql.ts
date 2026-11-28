/** r45: native-owned read-only heads shared across files owner adapters. The
 * statement bytes live in the Rust-owned contract `events-sets-head-sources.json`;
 * every call site still prepares and executes its own statement in its supplied
 * database/transaction with its own predicate/tail and binds. Advisory locks,
 * row locks, FOR UPDATE, LOCK TABLE, native writes and every r1-r44 helper are
 * untouched. */
import contract from "./events-sets-head-sources.json" with { type: "json" };

const heads = contract as {
  readonly attachmentEventsWhere: string;
  readonly attachmentSetsStarWhere: string;
};

export const ATTACHMENT_EVENTS_WHERE_HEAD_SQL = heads.attachmentEventsWhere;
export const ATTACHMENT_SETS_STAR_WHERE_HEAD_SQL = heads.attachmentSetsStarWhere;
