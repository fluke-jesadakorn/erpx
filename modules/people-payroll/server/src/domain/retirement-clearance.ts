import {hasPermission} from "@erpx/identity-access-ui/lib/permissions";import {clearancePermissions} from "@erpx/people-payroll-ui/lib/retirement";import type {PreparationTask} from "@erpx/people-payroll-ui/lib/people-lifecycle";import type {WorkContext} from '@erpx/identity-access-server/public/authorization';export function canCompleteClearance(actor:WorkContext,task:Pick<PreparationTask,"assignee_id"|"clearance_type">) {
 const kind=task.clearance_type;
 if(!kind || task.assignee_id!==actor.userId)return false;
 return (!clearancePermissions[kind] || hasPermission(actor,clearancePermissions[kind]!)) && (!["payroll","finance","directory"].includes(kind) || !actor.branchId && !actor.departmentId);
}
export function canReadClearanceEvidence(actor:WorkContext,task:Pick<PreparationTask,"assignee_id"|"clearance_type"|"completed_by">) {
 const kind=task.clearance_type;if(!kind)return false;
 const permission=clearancePermissions[kind];
 if(permission&&!hasPermission(actor,permission))return false;
 if(["payroll","finance","directory"].includes(kind)&&(actor.branchId||actor.departmentId))return false;
 // Clearance evidence belongs to one responsibility, not the whole People or
 // department audience.  A current permission is necessary but never enough:
 // only the assigned worker (or the worker whose completed decision is now
 // part of the immutable history) may read that task's evidence.
 return task.assignee_id===actor.userId || task.completed_by===actor.userId;
}
