import "server-only";
/** Byte-identical Legal item-list read prefix shared by owner adapter point reads.
 * Text only: every call site still prepares and executes with its own predicate and binds. */
export const legalItemSelectSql = "SELECT i.* FROM legal_items i WHERE ";
