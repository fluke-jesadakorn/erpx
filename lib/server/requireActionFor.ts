// Server-action-friendly wrapper around `requireAction`.
// Loads the actor, validates they match the expected id, then audits
// stage overrides.

import 'server-only';
import { loadActor, requireAction, GuardError, type ActorWithScope } from './guard';
import { query } from '../db';
import type { ActionName } from './sessionToken.types';

export interface RequireActionOpts {
  perm?: string;
  stage?: string;
  entityCtx?: {
    entityType: 'expense' | 'pr';
    entityId: number;
  };
}

export async function requireActionFor(
  expectedActorId: number,
  action: ActionName,
  opts: RequireActionOpts = {},
): Promise<{ actor: ActorWithScope; override: boolean }> {
  const actor = await loadActor();
  if (!actor) throw new GuardError('unauthorized', 401);
  if (actor.id !== expectedActorId) {
    throw new GuardError('actor mismatch', 403);
  }
  const res = await requireAction(actor, action, opts);
  if (!res.allowed) throw new GuardError(res.reason || 'forbidden', 403);
  if (res.override) await writeStageOverrideAudit(actor, action, opts);
  return { actor, override: res.override };
}

async function writeStageOverrideAudit(
  actor: ActorWithScope,
  action: ActionName,
  opts: RequireActionOpts,
): Promise<void> {
  if (!opts.stage) return;
  const entityType = action.includes('pr') ? 'pr' : action.includes('po') ? 'po' : 'expense';
  try {
    await query(
      `INSERT INTO stage_override_audit
        (actor_id, entity_type, entity_id, attempted_stage, required_role, actor_role, reason)
       VALUES ($1,$2,$3,$4,$5,$6,$7)`,
      [
        actor.id,
        entityType,
        0,
        opts.stage,
        opts.stage,
        actor.role_name,
        `out-of-stage ${action}`,
      ],
    );
  } catch (e) {
    console.warn('stage_override_audit insert failed:', e);
  }
}