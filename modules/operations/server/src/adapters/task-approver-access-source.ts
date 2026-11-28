import 'server-only';
/** Canonical task approver-access clause. Alias, approver column and value
 * placeholder are the only varying bytes; every rendering is byte-identical. */
export const approverJsonExistsSql = (alias:string,column:'start'|'completion',value:string) => `EXISTS(SELECT 1 FROM erpx_json_each(${alias}.${column}_approvers_json) WHERE value=${value})`;
export const OPERATIONS_TASK_START_APPROVER_SQL = approverJsonExistsSql('t','start','?');
export const OPERATIONS_TASK_COMPLETION_APPROVER_SQL = approverJsonExistsSql('t','completion','?');
