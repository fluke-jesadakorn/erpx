import 'server-only';
import { assignmentsProjectionCompact } from './assignment-projection-source';
/** Fixed explicit-column owning projections; retains the consumer's supplied statement snapshot. */
export function workflowFinalOwnerProjections(){return {
  "approval_participant_projection_assignments_aa_from": `FROM (${assignmentsProjectionCompact}) aa`,
  "work_role_audit_assignments_from": `FROM (${assignmentsProjectionCompact}) assignments`
} as const;}
