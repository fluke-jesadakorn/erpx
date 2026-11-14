/** Compatibility entry; its legacy implementation is tracked in ownership.json. */
export { taskFieldsSchema, taskSchema, projectSchema, taskActionSchema, taskDelegationSchema, taskDelegationRevokeSchema, listTasks, canConfigureTask, taskVersionGuard, getProject, listProjects, validateTaskConfiguration, configGuards, saveTask, taskWorkState, taskDetail, canAttachTask } from "@erpx/server-operations/adapters/staff-tasks";
export type { TaskRow, TaskDetail, ProjectRow, TaskWorkState } from "@erpx/server-operations/adapters/staff-tasks";
