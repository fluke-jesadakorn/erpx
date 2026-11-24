import 'server-only';
/** Canonical task approver-access clauses shared by Operations owner reads. */
export const OPERATIONS_TASK_START_APPROVER_SQL = `EXISTS(SELECT 1 FROM erpx_json_each(t.start_approvers_json) WHERE value=?)`;
export const OPERATIONS_TASK_COMPLETION_APPROVER_SQL = `EXISTS(SELECT 1 FROM erpx_json_each(t.completion_approvers_json) WHERE value=?)`;
