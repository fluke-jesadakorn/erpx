import 'server-only';
import { OPERATIONS_OWNER_PROJECTION_ID_HEAD } from './read-tail-r57-sources';
/** Fixed explicit-column owning projections; retains the consumer's supplied statement snapshot. */
export function peoplePayrollFinalOwnerProjections(){return {
  "operations_staff_tasks_facts_staff_task_delegations_d_from": `FROM (${OPERATIONS_OWNER_PROJECTION_ID_HEAD}"task_id","delegator_user_id","delegate_user_id","starts_at","expires_at","reason","status","revoked_by","revoked_at","version","created_at","updated_at","revoke_reason" FROM staff_task_delegations) d`
} as const;}
