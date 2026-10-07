/** Compatibility entry; its legacy implementation is tracked in ownership.json. */
export { taskFieldsSchema, taskSchema, projectSchema, taskActionSchema, taskDelegationSchema, taskDelegationRevokeSchema, listTasks, getTask, taskSourceReadGuard, canConfigureTask, delegateTask, revokeTaskDelegation, taskVersionGuard, getProject, listProjects, saveProject, validateTaskConfiguration, configGuards, saveTask, taskWorkState, taskDetail, transitionTask, canAttachTask } from "../../../staff-tasks";
export type { TaskRow, TaskDetail, ProjectRow, TaskWorkState } from "../../../staff-tasks";
