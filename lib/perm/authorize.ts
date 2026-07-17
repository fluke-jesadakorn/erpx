import 'server-only';
import { query } from '@folio-lib/db';
import {
  ADMIN_PERM,
  matchPerm,
  parsePerm,
} from './grammar';
import { evaluateAction, evaluatePolicy, getPolicy } from './policy';
import { check } from './relationship';
import type { AuthAction, Decision, ResourceRef } from './decision';

export interface ActorForAuth {
  id: number;
  permissions: string[];
  deptId?: string | null;
  level?: number;
  roleName?: string | null;
}

export interface AuthorizeCtx {
  totalAmount?: number | null;
  currentStage?: string | null;
}

interface QualifierContext {
  actor: ActorForAuth;
  resource: ResourceRef | undefined;
}

async function qualifierSatisfied(
  perm: string,
  qc: QualifierContext,
): Promise<{ ok: boolean; scope?: string }> {
  const parts = parsePerm(perm);
  if (!parts) return { ok: false };
  const q = parts.qualifier;
  if (!q || q === '*' || q === 'all') return { ok: true, scope: 'all' };

  const r = qc.resource ?? {};
  const actorId = qc.actor.id;
  const actorDept = qc.actor.deptId ?? null;

  if (q === 'self') {
    const owners = [r.submitterId, r.uploaderId, r.requesterId, r.ownerId]
      .filter((v): v is number => typeof v === 'number');
    return { ok: owners.includes(actorId), scope: 'self' };
  }
  if (q === 'dept') {
    if (!r.deptId || !actorDept) return { ok: false, scope: 'dept' };
    return { ok: r.deptId === actorDept, scope: 'dept' };
  }
  if (q === 'subtree') {
    if (!r.deptId || !actorDept) return { ok: false, scope: 'subtree' };
    return { ok: r.deptId === actorDept, scope: 'subtree' };
  }
  return { ok: r.deptId === q || actorDept === q, scope: 'qualifier' };
}

async function logDecision(
  actor: ActorForAuth,
  action: AuthAction,
  decision: Decision,
  resource?: ResourceRef,
): Promise<void> {
  try {
    await query(
      `INSERT INTO perm.decision_log
         (actor_user_id, action_kind, action_target, resource_type, resource_id,
          decision, reason, matched_perm, matched_policy, occurred_at)
       VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, now())`,
      [
        actor.id,
        action.kind,
        actionTargetString(action),
        resource?.type ?? null,
        resource?.id != null ? String(resource.id) : null,
        decision.allow ? 'allow' : 'deny',
        decision.reason,
        decision.matchedPerm ?? null,
        decision.matchedPolicy ?? null,
      ],
    );
  } catch {
    // ignore
  }
}

function actionTargetString(action: AuthAction): string {
  switch (action.kind) {
    case 'perm':              return `perm:${action.perm}`;
    case 'policy':            return `policy:${action.policy}`;
    case 'stage':             return `stage:${action.stage}`;
    case 'stage_with_policy': return `stage:${action.stage}|policy:${action.policy}`;
  }
}

export async function authorize(
  actor: ActorForAuth,
  action: AuthAction,
  resource?: ResourceRef,
  ctx: AuthorizeCtx = {},
): Promise<Decision> {
  if (matchPerm(actor.permissions, ADMIN_PERM)) {
    const d: Decision = { allow: true, reason: 'admin bypass', scope: 'all' };
    await logDecision(actor, action, d, resource);
    return d;
  }

  let decision: Decision;

  switch (action.kind) {
    case 'perm': {
      const q = await qualifierSatisfied(action.perm, { actor, resource });
      if (!q.ok) {
        decision = {
          allow: false,
          reason: `qualifier "${q.scope}" not satisfied for ${action.perm}`,
          matchedPerm: action.perm,
          scope: q.scope,
        };
      } else if (matchPerm(actor.permissions, action.perm)) {
        decision = {
          allow: true,
          reason: `granted ${action.perm}`,
          matchedPerm: action.perm,
          scope: q.scope,
        };
      } else {
        decision = {
          allow: false,
          reason: `missing permission "${action.perm}"`,
          matchedPerm: action.perm,
          scope: q.scope,
        };
      }
      break;
    }
    case 'policy': {
      decision = await evaluatePolicy(action.policy, actor, {
        resource,
        totalAmount: ctx.totalAmount ?? resource?.totalAmount ?? null,
        currentStage: ctx.currentStage ?? resource?.currentStage ?? null,
      });
      break;
    }
    case 'stage': {
      const stagePerm = `stage:${action.stage}:act::allow`;
      const q = await qualifierSatisfied(stagePerm, { actor, resource });
      const granted = q.ok && matchPerm(actor.permissions, stagePerm);
      if (granted) {
        decision = {
          allow: true,
          reason: `stage permission granted: ${stagePerm}`,
          matchedPerm: stagePerm,
          scope: q.scope,
        };
      } else {
        const policyId = `waybill.${action.stage}`;
        const policyDecision = await evaluatePolicy(policyId, actor, {
          resource,
          totalAmount: ctx.totalAmount ?? resource?.totalAmount ?? null,
          currentStage: ctx.currentStage ?? resource?.currentStage ?? null,
        });
        decision = policyDecision.allow
          ? policyDecision
          : {
              allow: false,
              reason:
                policyDecision.reason ||
                `missing stage permission and policy did not match`,
              matchedPerm: stagePerm,
              matchedPolicy: policyDecision.matchedPolicy,
              scope: q.scope,
            };
      }
      break;
    }
    case 'stage_with_policy': {
      const policyDecision = await evaluatePolicy(action.policy, actor, {
        resource,
        totalAmount: ctx.totalAmount ?? resource?.totalAmount ?? null,
        currentStage: ctx.currentStage ?? resource?.currentStage ?? null,
      });
      if (policyDecision.allow) {
        decision = policyDecision;
        break;
      }
      const stagePerm = `stage:${action.stage}:act::allow`;
      const q = await qualifierSatisfied(stagePerm, { actor, resource });
      const granted = q.ok && matchPerm(actor.permissions, stagePerm);
      decision = granted
        ? {
            allow: true,
            reason: `stage permission granted: ${stagePerm}`,
            matchedPerm: stagePerm,
            scope: q.scope,
          }
        : {
            allow: false,
            reason: policyDecision.reason || `missing ${stagePerm}`,
            matchedPerm: stagePerm,
            matchedPolicy: action.policy,
          };
      break;
    }
  }

  await logDecision(actor, action, decision, resource);
  return decision;
}

export async function authorizeObject(
  actor: ActorForAuth,
  action: AuthAction,
  objectKey: string,
  ctx: AuthorizeCtx = {},
): Promise<Decision> {
  const [type, idRaw] = objectKey.split(':', 2);
  const idNum = /^\d+$/.test(idRaw ?? '') ? parseInt(idRaw!, 10) : null;
  const resource: ResourceRef = { type, id: idRaw, ownerId: actor.id };

  if (action.kind === 'perm') {
    const ownerOk = await check(actor.id, 'owner', objectKey);
    if (ownerOk) {
      const d: Decision = { allow: true, reason: 'object owner', matchedPerm: action.perm, scope: 'self' };
      await logDecision(actor, action, d, resource);
      return d;
    }
  }
  return authorize(actor, action, resource, ctx);
}

export { evaluateAction, evaluatePolicy, getPolicy };
export type { Decision, ResourceRef, AuthAction };

export async function hasPermissionAsync(
  actor: ActorForAuth,
  perm: string,
  resource?: ResourceRef,
): Promise<boolean> {
  const d = await authorize(actor, { kind: 'perm', perm }, resource);
  return d.allow;
}
