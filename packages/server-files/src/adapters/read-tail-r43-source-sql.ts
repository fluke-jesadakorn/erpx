/** r43: native-owned read-only heads shared across files owner adapters. The
 * statement bytes live in the Rust-owned contract `field-owner-head-sources.json`;
 * every call site still prepares and executes its own statement in its supplied
 * transaction with its own prefix, owner/WHERE tail and binds. Advisory locks,
 * row locks, LOCK TABLE, native writes and every r1-r42 helper are untouched. */
import contract from "./field-owner-head-sources.json" with { type: "json" };

const heads = contract as {
  readonly fieldUploadAncestor: string;
  readonly fieldSetUpload: string;
  readonly attachmentIdAnd: string;
};

export const FIELD_UPLOAD_ANCESTOR_EXISTS_SQL = heads.fieldUploadAncestor;
export const FIELD_SET_UPLOAD_EXISTS_HEAD_SQL = heads.fieldSetUpload;
export const ATTACHMENT_ID_AND_HEAD_SQL = heads.attachmentIdAnd;
