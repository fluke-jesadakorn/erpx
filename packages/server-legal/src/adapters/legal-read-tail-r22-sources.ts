import "server-only";
/** Byte-identical latest-approved-cycle read shared by Legal owner adapters (round 22).
 * Text only: every call site still prepares and executes its own projection with unchanged binds. */
export const legalApprovedCycleSql = (columns: string) =>
  `SELECT ${columns} FROM legal_approvals WHERE item_id=? AND decision='approved' ORDER BY cycle DESC LIMIT 1`;
