/** Native-owned scan-pending bind witness shared by owner read-tail adapters.
 * Text only: the statement token lives in the Rust-owned contract
 * `scan-pending-head-sources.json`; every call site still prepares and executes
 * its own statement in its supplied transaction with its own binds, and no SQL
 * text is accepted from a caller or cached here. */
import contract from "./scan-pending-head-sources.json" with { type: "json" };

const head = (contract as { readonly scanPending: string }).scanPending;

export const scanPendingSql = (attachmentRef: string) =>
  head.replace("{attachmentRef}", attachmentRef);
