/** r44: native-owned read-only witness heads shared across files owner
 * adapters. The statement bytes live in the Rust-owned contract
 * `witness-head-sources.json`; every call site still prepares and executes its
 * own statement in its supplied database/transaction with its own prefix,
 * alias, state tail and binds. Advisory locks, row locks, LOCK TABLE and native
 * writes stay at their call sites; no LOCK TABLE statement is introduced here. */
import contract from "./witness-head-sources.json" with { type: "json" };

const heads = contract as {
  readonly uploadIntentsExists: string;
  readonly attachmentStarId: string;
  readonly scanAttemptsOrExists: string;
};

export const UPLOAD_INTENTS_EXISTS_HEAD_SQL = heads.uploadIntentsExists;
export const ATTACHMENT_STAR_ID_HEAD_SQL = heads.attachmentStarId;
export const SCAN_ATTEMPTS_OR_EXISTS_HEAD_SQL = heads.scanAttemptsOrExists;
