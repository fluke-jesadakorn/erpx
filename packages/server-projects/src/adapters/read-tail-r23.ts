import {assignmentReadSql} from "@erpx/server-identity_access/public/record-source-authority";
import {projectAssignmentScopeSql} from "@erpx/server-projects/adapters/package-obligations";
/** Byte-identical current-assignment authority read shared by the Project
 * purchase and metric gates. Alias, permission and the closed `multiline`
 * separator are the only degrees of freedom; each emitted statement stays
 * exactly as its former inline literal. No SQL comes from a caller. */
export const projectAssignmentAuthorityReadSql = (alias: string, permission: string, multiline = false) =>
  `SELECT ${assignmentReadSql(alias,permission)} allowed FROM${multiline ? "\n " : " "}${projectAssignmentScopeSql(alias)}`;
