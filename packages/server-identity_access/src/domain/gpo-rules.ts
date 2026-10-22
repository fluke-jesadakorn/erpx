import type {WorkContext} from '@erpx/server-identity_access/public/authorization';
import {hasPermission} from "@erpx/feature-identity_access/lib/permissions";
import {z} from 'zod';
import type {GpoRule,GpoRuleTarget} from "@erpx/feature-organization/lib/gpo-rules";
export type GpoRuleRow = {
  id: string; company_id: string; target_kind: GpoRuleTarget["kind"]; target_id: string;
  version: number; revision: number; connection_id: string; gpo_id: string;
  user_filter_group_id: string; computer_target_ou_id: string; active: boolean;
  directory_config_status: GpoRule["directoryStatus"]; directory_config_error: string | null;
  legacy_position_id: string | null; target_name: string;
};
export type EffectiveRuleRow = GpoRuleRow & { position_id: string };
export type Position = { id: string; title: string; branch_id: string | null; department_id: string | null; level_id: string; version: number };
export function canManageGpoRules(actor: WorkContext) {
  return actor.role === "it_admin" && actor.assignmentScope === "holding" && !actor.branchId && !actor.departmentId && hasPermission(actor, "iam.directory.apply");
}
export function presentGpoRule(row: GpoRuleRow): GpoRule {
  return { id: row.id, target: { kind: row.target_kind, id: row.target_id }, targetName: row.target_name,
    version: row.version, revision: row.revision, connectionId: row.connection_id, gpoId: row.gpo_id,
    userFilterGroupId: row.user_filter_group_id, computerTargetOuId: row.computer_target_ou_id,
    active: row.active, directoryStatus: row.directory_config_status, directoryError: row.directory_config_error,
    legacyPositionId: row.legacy_position_id };
}
export const gpoRuleMatchesPosition = (rule: Pick<GpoRuleRow, "target_kind" | "target_id" | "active">, position: Pick<Position,"id"|"level_id"|"department_id">) => rule.active &&
  rule.target_kind === "position" && rule.target_id === position.id;
export const gpoRuleSourceId = (ruleId: string, engagementId: string, legacy: boolean) => legacy ? `position-gpo:${engagementId}` : `gpo-rule:${JSON.stringify([ruleId, engagementId])}`;
export const tokenSchema=z.string().regex(/^[a-f0-9]{64}$/);
