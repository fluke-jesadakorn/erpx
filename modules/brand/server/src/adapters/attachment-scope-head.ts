/** Native-owned head for the Brand private-attachment current-scope probe. The
 * exact `VALUES` scope envelope lives in this owner's JSON contract, bound into
 * Rust with `include_str!` and exposed by `public::attachment_scope_head`.
 * Identity Access still owns the predicate inside `where` and its binds; this
 * helper carries no statement token and accepts no caller SQL. */
import contract from "./attachment-scope-head-sources.json" with { type: "json" };

const heads = contract as { readonly scopeHead: string };

/** Render the exact current-assignment scope probe. `where` is the Identity
 * Access-owned predicate; binds stay at the caller in company/branch/department
 * order followed by the owner args. */
export const brandAttachmentScopeProbeSql = (where: string) =>
  `${heads.scopeHead}${where}`;
