/** r48: native-owned read-only SQL heads shared across files owner adapters.
 * The statement bytes live in the Rust-owned contract
 * `settlement-head-sources.json`; every call site still prepares and executes
 * its own statement in its supplied transaction with its own alias, column/WHERE
 * tail and binds. Advisory locks, row locks, LOCK TABLE, native writes and
 * every r1-r47 helper are left untouched. */
import contract from "./settlement-head-sources.json" with { type: "json" };

const heads = contract as {
  readonly attachmentExists: string;
  readonly messagingSettlement: string;
  readonly packageFileLinkTarget: string;
};

export const ATTACHMENT_EXISTS_HEAD_SQL = heads.attachmentExists;
export const MESSAGING_SETTLEMENT_HEAD_SQL = heads.messagingSettlement;
export const PACKAGE_FILE_LINK_TARGET_HEAD_SQL = heads.packageFileLinkTarget;
