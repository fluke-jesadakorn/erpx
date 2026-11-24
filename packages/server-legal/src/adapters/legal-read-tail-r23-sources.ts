import "server-only";
/** Byte-identical Legal hold-active point read shared by destruction and retention owners (round 23).
 * Text only: the caller chooses the fixed inequality token; every call site still binds its own item id. */
export const legalHoldActiveSql = (inequality: "!=" | "<>") =>
  `SELECT 1 FROM legal_hold_targets t JOIN legal_holds h ON h.id=t.hold_id WHERE t.item_id=? AND h.state${inequality}'released'`;
