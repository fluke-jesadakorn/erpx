/** Native-owned orphan-preservation witnesses shared by the read-tail call
 * sites. The statement bytes live in the Rust-owned contract
 * `orphan-preservation-head-sources.json`; every call site still prepares and
 * executes its own statement in its supplied database/transaction with its own
 * bind, and no result is cached here. */
import contract from "./orphan-preservation-head-sources.json" with { type: "json" };

const heads = contract as { readonly presence: string; readonly pending: string };

export const ORPHAN_PRESERVATION_PRESENT_SQL = heads.presence;
export const ORPHAN_PRESERVATION_PENDING_SQL = heads.pending;
