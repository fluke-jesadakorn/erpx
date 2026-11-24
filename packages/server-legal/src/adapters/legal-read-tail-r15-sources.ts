import "server-only";
/** Byte-identical later-approval guard SQL shared by Legal executed-copy assertions.
 * Text only: every assertion site still prepares and executes with its own binds. */
export const legalLaterApprovalGuardSql = "NOT EXISTS(SELECT 1 FROM legal_approvals WHERE item_id=? AND cycle>?)";
