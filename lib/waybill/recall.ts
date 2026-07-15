import 'server-only';
import { loadActivePermSession } from '../perm/auth';
import { hasPermission, ADMIN_PERM } from '../perm/auth-client';
import { matchPerm } from '../perm/grammar';

const RECALL_PERM = 'finance:waybill:recall::allow';
const RECALL_ROLES = new Set(['cfo', 'ceo', 'admin', 'finance']);

export async function canActorReCall(actorRole: string | null): Promise<boolean> {
  const out = await loadActivePermSessionFromEnv();
  if (out) {
    const perms = out.session.permissions ?? [];
    if (perms.includes(ADMIN_PERM)) return true;
    if (matchPerm(perms, RECALL_PERM)) return true;
    if (RECALL_ROLES.has(out.session.user.role ?? '')) return true;
    if (actorRole && RECALL_ROLES.has(actorRole)) return true;
    return false;
  }
  return !!actorRole && RECALL_ROLES.has(actorRole);
}

async function loadActivePermSessionFromEnv(): Promise<Awaited<ReturnType<typeof loadActivePermSession>>> {
  const { headers } = await import('next/headers');
  const h = await headers();
  return loadActivePermSession(new Request('http://internal', { headers: h as unknown as HeadersInit }));
}

export interface ReCallInput {
  waybillId: string;
  targetStage: string;
  actorId: number;
  actorRole: string;
  reason?: string;
}

export async function reCallWaybillAction(_input: ReCallInput): Promise<{ ok: boolean; error?: string }> {
  return { ok: false, error: 'deprecated: use canActorReCall + matchPerm' };
}
