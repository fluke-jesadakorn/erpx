import 'server-only';

import { controlReadQuery, withTransaction } from '@/db';
import type { PlatformRoleAssignment } from './types';
import { enqueueBridgeEvent } from './bridge-write';
import { validateChildAssignmentWindow } from './rbac-contract';
import {
  assertStepUpEvidence,
  STEP_UP_OPERATIONS,
  type StepUpEvidence,
} from './step-up';

export type AssignmentRiskTier = 'platform_root' | 'administrator' | 'high' | 'standard' | 'contractor' | 'support' | 'emergency';
export interface AssignmentSubtreeScope {
  companyIds?: string[];
  branchIds?: string[];
}

const MAX_MINUTES: Record<AssignmentRiskTier, number> = {
  platform_root: 90 * 24 * 60,
  administrator: 90 * 24 * 60,
  high: 90 * 24 * 60,
  standard: 365 * 24 * 60,
  contractor: 30 * 24 * 60,
  support: 8 * 60,
  emergency: 60,
};

export function maximumAssignmentMinutes(riskTier: AssignmentRiskTier): number {
  return MAX_MINUTES[riskTier];
}

const HIGH_RISK_TIERS = new Set<AssignmentRiskTier>(['platform_root', 'administrator', 'high']);

type RoleEventQuery = <R extends import('pg').QueryResultRow = import('pg').QueryResultRow>(
  text: string,
  params?: unknown[],
) => Promise<import('pg').QueryResult<R>>;

async function appendRoleAssignmentEvent(
  tx: RoleEventQuery,
  args: {
    assignmentId: string;
    eventType: 'granted' | 'renewed' | 'revoked';
    assignmentVersion: number;
    actor: string;
    reason: string;
    payload: Record<string, unknown>;
  },
): Promise<void> {
  const idempotencyKey = `folio-role-assignment-${args.assignmentId}-${args.eventType}-${args.assignmentVersion}`;
  await tx(
    `INSERT INTO platform.role_assignment_events
       (assignment_id, event_type, assignment_version, actor, reason, payload)
     VALUES ($1, $2, $3, $4, $5, $6)`,
    [args.assignmentId, args.eventType, args.assignmentVersion, args.actor, args.reason, JSON.stringify(args.payload)],
  );
  await enqueueBridgeEvent(tx, {
    eventType: `role_assignment_${args.eventType}`,
    aggregateType: 'role_assignment',
    aggregateId: args.assignmentId,
    aggregateVersion: args.assignmentVersion,
    idempotencyKey,
    payload: {
      assignmentId: args.assignmentId,
      eventType: args.eventType,
      assignmentVersion: args.assignmentVersion,
      actor: args.actor,
      reason: args.reason,
      ...args.payload,
    },
  });
}

function requiresStepUp(riskTier: AssignmentRiskTier): boolean {
  return HIGH_RISK_TIERS.has(riskTier);
}

function persistedScope(value: unknown): Record<string, unknown> {
  if (value && typeof value === 'object' && !Array.isArray(value)) return value as Record<string, unknown>;
  if (typeof value === 'string') {
    try {
      const parsed = JSON.parse(value) as unknown;
      if (parsed && typeof parsed === 'object' && !Array.isArray(parsed)) return parsed as Record<string, unknown>;
    } catch {
      // Treat malformed legacy scope metadata as an empty scope. PostgreSQL JSONB normally prevents this.
    }
  }
  return {};
}

export function validateAssignmentWindow(args: {
  riskTier: AssignmentRiskTier;
  validFrom: Date;
  validUntil: Date;
  grantorActorId?: number;
  stepUpEvidence?: StepUpEvidence | null;
  operation?: typeof STEP_UP_OPERATIONS.createRole | typeof STEP_UP_OPERATIONS.renewRole;
}): void {
  const durationMinutes = (args.validUntil.getTime() - args.validFrom.getTime()) / 60000;
  if (!Number.isFinite(durationMinutes) || durationMinutes <= 0) throw new Error('assignment window must end after it starts');
  if (durationMinutes > maximumAssignmentMinutes(args.riskTier)) throw new Error(`assignment exceeds the ${maximumAssignmentMinutes(args.riskTier)} minute ${args.riskTier} maximum`);
  if (requiresStepUp(args.riskTier)) {
    if (!args.grantorActorId) throw new Error('server-verified step-up MFA evidence is required');
    assertStepUpEvidence(args.stepUpEvidence, {
      actorId: args.grantorActorId,
      action: args.operation ?? STEP_UP_OPERATIONS.createRole,
    });
  }
}

function scopeContains(
  parent: { holding_id: string | null; company_id: string | null; branch_id: string | null; subtree_scope: AssignmentSubtreeScope },
  child: { holding_id: string | null; company_id: string | null; branch_id: string | null; subtree_scope: AssignmentSubtreeScope },
): boolean {
  let structural = true;
  if (parent.holding_id || parent.company_id || parent.branch_id) {
    if (parent.branch_id) structural = parent.branch_id === child.branch_id;
    else if (parent.company_id) structural = parent.company_id === child.company_id && parent.holding_id === child.holding_id;
    else structural = parent.holding_id === child.holding_id;
  }
  if (!structural) return false;
  const companyIds = parent.subtree_scope.companyIds ?? [];
  const branchIds = parent.subtree_scope.branchIds ?? [];
  if (branchIds.length > 0) return Boolean(child.branch_id && branchIds.includes(child.branch_id));
  if (companyIds.length > 0) return Boolean(child.company_id && companyIds.includes(child.company_id));
  return true;
}

function mapAssignment(row: Record<string, unknown>): PlatformRoleAssignment {
  const scopeLabel = [row.holding_name, row.company_name, row.branch_name].filter(Boolean).join(' / ') || 'Platform-wide';
  const scope = persistedScope(row.subtree_scope);
  const renewedFrom = typeof scope.renewalOfAssignmentId === 'string' ? scope.renewalOfAssignmentId : null;
  return {
    id: String(row.id), principal_ref: String(row.principal_ref), principal_type: String(row.principal_type), role_key: String(row.role_key), functional_domain: String(row.functional_domain), authority_level: Number(row.authority_level ?? 99), scope_label: scopeLabel,
    valid_from: String(row.valid_from), valid_until: String(row.valid_until), risk_tier: String(row.risk_tier), grant_reason: String(row.grant_reason), parent_assignment_id: row.parent_assignment_id == null ? null : String(row.parent_assignment_id), assignment_version: Number(row.assignment_version ?? 1), effective: Boolean(row.effective), renewed_from_assignment_id: renewedFrom,
  };
}

export async function createRoleAssignment(args: {
  principalRef: string;
  principalType: 'platform_user' | 'tenant_user' | 'service';
  roleKey: string;
  permissions: string[];
  functionalDomain: string;
  authorityLevel: number;
  holdingId?: string | null;
  companyId?: string | null;
  branchId?: string | null;
  subtreeScope?: AssignmentSubtreeScope;
  validFrom: Date;
  validUntil: Date;
  timezone: string;
  riskTier: AssignmentRiskTier;
  grantReason: string;
  ticketReference?: string | null;
  contractReference?: string | null;
  grantor: string;
  grantorActorId: number;
  approver?: string | null;
  renewalReviewer?: string | null;
  parentAssignmentId?: string | null;
  stepUpEvidence?: StepUpEvidence | null;
}): Promise<PlatformRoleAssignment> {
  validateAssignmentWindow(args);
  const approver = requiresStepUp(args.riskTier) ? args.stepUpEvidence?.approverRef ?? null : args.approver ?? null;
  return withTransaction(async (tx) => {
    if (args.parentAssignmentId) {
      const parent = await tx<{ holding_id: string | null; company_id: string | null; branch_id: string | null; subtree_scope: AssignmentSubtreeScope; valid_from: string; valid_until: string; revoked_at: string | null }>(
        `SELECT holding_id::text, company_id::text, branch_id::text, subtree_scope, valid_from::text, valid_until::text, revoked_at::text FROM platform.role_assignments WHERE id = $1 FOR SHARE`,
        [args.parentAssignmentId],
      );
      const parentRow = parent.rows[0];
      if (!parentRow) throw new Error('parent assignment not found');
      validateChildAssignmentWindow({
        parentValidFrom: new Date(parentRow.valid_from),
        parentValidUntil: new Date(parentRow.valid_until),
        parentRevokedAt: parentRow.revoked_at ? new Date(parentRow.revoked_at) : null,
        childValidFrom: args.validFrom,
        childValidUntil: args.validUntil,
      });
      const childScope = { holding_id: args.holdingId ?? null, company_id: args.companyId ?? null, branch_id: args.branchId ?? null, subtree_scope: args.subtreeScope ?? {} };
      if (!scopeContains(parentRow, childScope)) throw new Error('child scope exceeds parent assignment scope');
      if (args.validUntil.getTime() > new Date(parentRow.valid_until).getTime()) throw new Error('child assignment cannot outlive its parent');
    }
    const result = await tx<Record<string, unknown>>(
      `INSERT INTO platform.role_assignments
        (principal_ref, principal_type, role_key, permissions, functional_domain, authority_level,
         holding_id, company_id, branch_id, subtree_scope, valid_from, valid_until, timezone, risk_tier,
         grant_reason, ticket_reference, contract_reference, grantor, approver, renewal_reviewer,
         parent_assignment_id)
       VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, $20, $21)
       RETURNING id::text, principal_ref, principal_type, role_key, functional_domain, authority_level,
                 valid_from::text, valid_until::text, risk_tier, grant_reason, parent_assignment_id::text,
                 assignment_version, subtree_scope, (valid_from <= now() AND valid_until > now() AND revoked_at IS NULL) AS effective`,
      [args.principalRef, args.principalType, args.roleKey, args.permissions, args.functionalDomain, args.authorityLevel, args.holdingId ?? null, args.companyId ?? null, args.branchId ?? null, JSON.stringify(args.subtreeScope ?? {}), args.validFrom, args.validUntil, args.timezone, args.riskTier, args.grantReason, args.ticketReference ?? null, args.contractReference ?? null, args.grantor, approver, args.renewalReviewer ?? null, args.parentAssignmentId ?? null],
    );
    const assignment = mapAssignment(result.rows[0]);
    await appendRoleAssignmentEvent(tx, {
      assignmentId: assignment.id,
      eventType: 'granted',
      assignmentVersion: assignment.assignment_version,
      actor: args.grantor,
      reason: args.grantReason,
      payload: {
        principalRef: assignment.principal_ref,
        roleKey: assignment.role_key,
        riskTier: assignment.risk_tier,
        parentAssignmentId: assignment.parent_assignment_id,
      },
    });
    return assignment;
  });
}

export async function listRoleAssignments(limit = 100): Promise<PlatformRoleAssignment[]> {
  const result = await controlReadQuery<Record<string, unknown>>(
    `SELECT ra.id::text, ra.principal_ref, ra.principal_type, ra.role_key, ra.functional_domain,
            ra.authority_level, ra.valid_from::text, ra.valid_until::text, ra.risk_tier,
            ra.grant_reason, ra.parent_assignment_id::text, ra.assignment_version, ra.subtree_scope,
            (ra.valid_from <= now() AND ra.valid_until > now() AND ra.revoked_at IS NULL) AS effective,
            h.name AS holding_name, co.legal_name AS company_name, b.name AS branch_name
       FROM platform.role_assignments ra
       LEFT JOIN platform.holdings h ON h.id = ra.holding_id
       LEFT JOIN platform.companies co ON co.id = ra.company_id
       LEFT JOIN platform.branches b ON b.id = ra.branch_id
      ORDER BY ra.valid_until ASC
      LIMIT $1`,
    [limit],
  );
  return result.rows.map(mapAssignment);
}

export async function renewRoleAssignment(args: {
  id: string;
  validFrom: Date;
  validUntil: Date;
  grantor: string;
  grantorActorId: number;
  approver?: string | null;
  stepUpEvidence?: StepUpEvidence | null;
}): Promise<PlatformRoleAssignment> {
  return withTransaction(async (tx) => {
    const current = await tx<Record<string, unknown>>(`SELECT * FROM platform.role_assignments WHERE id = $1 FOR UPDATE`, [args.id]);
    const row = current.rows[0];
    if (!row) throw new Error('assignment not found');
    const riskTier = String(row.risk_tier) as AssignmentRiskTier;
    if (row.revoked_at) throw new Error('role assignment is already revoked');
    validateAssignmentWindow({ riskTier, validFrom: args.validFrom, validUntil: args.validUntil, grantorActorId: args.grantorActorId, stepUpEvidence: args.stepUpEvidence, operation: STEP_UP_OPERATIONS.renewRole });
    const approver = requiresStepUp(riskTier) ? args.stepUpEvidence?.approverRef ?? null : args.approver ?? null;
    if (row.parent_assignment_id) {
      const parent = await tx<{ valid_from: string; valid_until: string; revoked_at: string | null }>(
        `SELECT valid_from::text, valid_until::text, revoked_at::text FROM platform.role_assignments WHERE id = $1`,
        [row.parent_assignment_id],
      );
      if (parent.rows[0]) {
        validateChildAssignmentWindow({
          parentValidFrom: new Date(parent.rows[0].valid_from),
          parentValidUntil: new Date(parent.rows[0].valid_until),
          parentRevokedAt: parent.rows[0].revoked_at ? new Date(parent.rows[0].revoked_at) : null,
          childValidFrom: args.validFrom,
          childValidUntil: args.validUntil,
        });
      }
    }
    const renewedScope = {
      ...persistedScope(row.subtree_scope),
      renewalOfAssignmentId: String(row.id),
      renewalOfVersion: Number(row.assignment_version ?? 1),
    };
    await tx(
      `UPDATE platform.role_assignments SET revoked_at = now() WHERE id = $1 AND revoked_at IS NULL`,
      [args.id],
    );
    await appendRoleAssignmentEvent(tx, {
      assignmentId: String(row.id),
      eventType: 'revoked',
      assignmentVersion: Number(row.assignment_version ?? 1),
      actor: args.grantor,
      reason: 'role assignment renewed',
      payload: { renewalOfAssignmentId: String(row.id), renewalOfVersion: Number(row.assignment_version ?? 1) },
    });
    const result = await tx<Record<string, unknown>>(
      `INSERT INTO platform.role_assignments
        (principal_ref, principal_type, role_key, permissions, functional_domain, authority_level,
         holding_id, company_id, branch_id, subtree_scope, valid_from, valid_until, timezone, risk_tier,
         grant_reason, ticket_reference, contract_reference, grantor, approver, renewal_reviewer,
         parent_assignment_id, assignment_version)
       VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, $20, $21, $22)
       RETURNING id::text, principal_ref, principal_type, role_key, functional_domain, authority_level,
                 valid_from::text, valid_until::text, risk_tier, grant_reason, parent_assignment_id::text,
                 assignment_version, subtree_scope, (valid_from <= now() AND valid_until > now() AND revoked_at IS NULL) AS effective`,
      [row.principal_ref, row.principal_type, row.role_key, row.permissions, row.functional_domain, row.authority_level, row.holding_id, row.company_id, row.branch_id, JSON.stringify(renewedScope), args.validFrom, args.validUntil, row.timezone, riskTier, row.grant_reason, row.ticket_reference, row.contract_reference, args.grantor, approver, row.renewal_reviewer, row.parent_assignment_id, Number(row.assignment_version) + 1],
    );
    const assignment = mapAssignment(result.rows[0]);
    await appendRoleAssignmentEvent(tx, {
      assignmentId: assignment.id,
      eventType: 'renewed',
      assignmentVersion: assignment.assignment_version,
      actor: args.grantor,
      reason: 'role assignment renewed',
      payload: { renewalOfAssignmentId: String(row.id), renewalOfVersion: Number(row.assignment_version ?? 1) },
    });
    return assignment;
  });
}

export async function revokeRoleAssignment(args: {
  id: string;
  revokedBy: string;
  revokedByActorId: number;
  stepUpEvidence?: StepUpEvidence | null;
}): Promise<{ id: string; revokedAt: string }> {
  return withTransaction(async (tx) => {
    const current = await tx<{ id: string; risk_tier: string; revoked_at: string | null; assignment_version: number; role_key: string }>(
      `SELECT id::text, risk_tier, revoked_at::text, assignment_version, role_key FROM platform.role_assignments WHERE id = $1 FOR UPDATE`,
      [args.id],
    );
    const row = current.rows[0];
    if (!row) throw new Error('assignment not found');
    if (row.revoked_at) throw new Error('role assignment is already revoked');
    const riskTier = row.risk_tier as AssignmentRiskTier;
    if (requiresStepUp(riskTier)) {
      assertStepUpEvidence(args.stepUpEvidence, { actorId: args.revokedByActorId, action: STEP_UP_OPERATIONS.revokeRole });
    }
    const result = await tx<{ id: string; revoked_at: string }>(
      `UPDATE platform.role_assignments SET revoked_at = now() WHERE id = $1 AND revoked_at IS NULL RETURNING id::text, revoked_at::text`,
      [args.id],
    );
    if (!result.rows[0]) throw new Error('role assignment is already revoked');
    await appendRoleAssignmentEvent(tx, {
      assignmentId: result.rows[0].id,
      eventType: 'revoked',
      assignmentVersion: Number(row.assignment_version),
      actor: args.revokedBy,
      reason: `role ${row.role_key} revoked`,
      payload: { roleKey: row.role_key },
    });
    return { id: result.rows[0].id, revokedAt: result.rows[0].revoked_at };
  });
}
