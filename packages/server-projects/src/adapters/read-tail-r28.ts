/** Byte-identical current-assignment viewer select-head shared by the Project
 * product-entry role candidate, the scheduled alert authority viewer and the
 * alert-scope LATERAL viewer. The clock expression and the trailing FROM/WHERE
 * stay at each caller; only the fixed projection below is shared. No SQL comes
 * from a caller. */
export const assignmentViewerSelect = `SELECT g.id assignment_id,g.user_id,g.role,`;
