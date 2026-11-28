/** Native-owned owner projections shared by the read-tail projection-source
 * modules. The statement bytes live in the Rust-owned contract
 * `owner-projection-head-sources.json`; consumers retain their own guards,
 * aliases and binds and execute in their supplied transaction. No SQL or
 * relation name is accepted from a caller and no result is cached here. */
import contract from "./owner-projection-head-sources.json" with { type: "json" };

const heads = contract as {
  readonly attachments: string;
  readonly attachmentLinks: string;
  readonly attachmentSets: string;
};

export const ATTACHMENTS_OWNER_PROJECTION_SQL = heads.attachments;
export const ATTACHMENT_LINKS_OWNER_PROJECTION_SQL = heads.attachmentLinks;
export const ATTACHMENT_SETS_OWNER_PROJECTION_SQL = heads.attachmentSets;
