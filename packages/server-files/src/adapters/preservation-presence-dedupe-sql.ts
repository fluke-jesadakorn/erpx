/** Byte-identical orphan-preservation presence probe shared by the read-tail
 * call sites. Text only: every call site still prepares and executes its own
 * statement in its supplied database/transaction and no result is cached here. */
export const ORPHAN_PRESERVATION_PRESENT_SQL = "SELECT to_regclass('attachment_orphan_preservations') present";
