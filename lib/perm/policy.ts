import 'server-only';
import { query } from '@folio-lib/db';
import type { Decision, ResourceRef } from './decision';
import { ADMIN_PERM, matchPerm, parsePerm } from './grammar';

export interface PolicyRule {
  when?: string;
  require?: string;
  allow?: string;
  deny?: string;
  reason?: string;
}

export interface Policy {
  id: string;
  name: string;
  description?: string | null;
  action: string;
  enabled: boolean;
  ast: { rules: PolicyRule[] };
}

interface ActorLike {
  id: number;
  permissions: string[];
  deptId?: string | null;
  level?: number;
  roleName?: string | null;
}

interface EvalCtx {
  resource?: ResourceRef;
  totalAmount?: number | null;
  currentStage?: string | null;
}

let policyCache: Map<string, Policy> | null = null;
let policyCacheLoadedAt = 0;
const POLICY_TTL_MS = 5_000;

async function loadPolicies(): Promise<Map<string, Policy>> {
  if (policyCache && Date.now() - policyCacheLoadedAt < POLICY_TTL_MS) {
    return policyCache;
  }
  const { rows } = await query<{
    id: string;
    name: string;
    description: string | null;
    action: string;
    enabled: boolean;
    ast: Policy['ast'];
  }>(
    `SELECT id, name, description, action, enabled, ast
       FROM perm.policies
      WHERE enabled IS NOT FALSE`,
  );
  const m = new Map<string, Policy>();
  for (const r of rows) {
    m.set(r.id, {
      id: r.id,
      name: r.name,
      description: r.description,
      action: r.action,
      enabled: r.enabled,
      ast: r.ast ?? { rules: [] },
    });
  }
  policyCache = m;
  policyCacheLoadedAt = Date.now();
  return m;
}

export async function getPolicy(id: string): Promise<Policy | null> {
  const m = await loadPolicies();
  return m.get(id) ?? null;
}

export async function listPolicies(): Promise<Policy[]> {
  const m = await loadPolicies();
  return Array.from(m.values());
}

export function invalidatePolicyCache(): void {
  policyCache = null;
  policyCacheLoadedAt = 0;
}

function checkExpr(
  expr: string,
  actor: ActorLike,
  ctx: EvalCtx,
): boolean {
  const r = ctx.resource ?? {};
  const totalAmount = ctx.totalAmount ?? r.totalAmount ?? null;
  const currentStage = ctx.currentStage ?? r.currentStage ?? null;

  const vars: Record<string, unknown> = {
    actor: {
      id: actor.id,
      dept_id: actor.deptId ?? null,
      level: actor.level ?? null,
      role_name: actor.roleName ?? null,
    },
    resource: r,
    total_amount: totalAmount,
    total_amount_thb: totalAmount,
    current_stage: currentStage,
    submitter: {
      id: r.submitterId ?? null,
      dept_id: r.submitterDeptId ?? null,
      level: r.submitterLevel ?? null,
    },
  };

  const fnSrc = `"use strict"; return (${expr});`;
  try {
    const fn = new Function('vars', `with (vars) { ${fnSrc} }`);
    return Boolean(fn(vars));
  } catch {
    return false;
  }
}

export async function evaluatePolicy(
  policyId: string,
  actor: ActorLike,
  ctx: EvalCtx = {},
): Promise<Decision> {
  if (matchPerm(actor.permissions, ADMIN_PERM)) {
    return { allow: true, reason: 'admin bypass', matchedPolicy: policyId };
  }

  const policy = await getPolicy(policyId);
  if (!policy) {
    return { allow: false, reason: `policy "${policyId}" not found`, matchedPolicy: policyId };
  }

  for (const rule of policy.ast.rules) {
    if (rule.when && !checkExpr(rule.when, actor, ctx)) continue;
    if (rule.deny) {
      const denyPerm = String(rule.deny);
      if (matchPerm(actor.permissions, denyPerm)) {
        return {
          allow: false,
          reason: rule.reason ?? `policy "${policyId}" denies (${denyPerm})`,
          matchedPolicy: policyId,
          matchedPerm: denyPerm,
        };
      }
      continue;
    }
    if (rule.allow) {
      const allowPerm = String(rule.allow);
      if (matchPerm(actor.permissions, allowPerm)) {
        return {
          allow: true,
          reason: rule.reason ?? `policy "${policyId}" allows (${allowPerm})`,
          matchedPolicy: policyId,
          matchedPerm: allowPerm,
        };
      }
      if (rule.require) {
        const requirePerm = String(rule.require);
        if (!matchPerm(actor.permissions, requirePerm)) continue;
      }
    }
  }

  return {
    allow: false,
    reason: `policy "${policyId}" did not match any rule`,
    matchedPolicy: policyId,
  };
}

export async function evaluateAction(
  actionKey: string,
  actor: ActorLike,
  ctx: EvalCtx = {},
): Promise<Decision> {
  const policies = await loadPolicies();
  for (const p of policies.values()) {
    if (p.action !== actionKey) continue;
    const d = await evaluatePolicy(p.id, actor, ctx);
    if (d.allow) return d;
  }
  return {
    allow: false,
    reason: `no enabled policy matches action "${actionKey}"`,
  };
}

export function permForAction(actionKey: string): string | null {
  const idx = actionKey.indexOf(':');
  if (idx < 0) return null;
  return `${actionKey}::allow`;
}

export function actionFromPerm(perm: string): string | null {
  const parts = parsePerm(perm);
  if (!parts) return null;
  return `${parts.domain}:${parts.subject}:${parts.verb}`;
}
