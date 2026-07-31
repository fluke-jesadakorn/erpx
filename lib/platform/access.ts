import 'server-only';

import { NextResponse } from 'next/server';
import { hasPermission, loadActivePermSession } from '@/perm/server';
import { loadActor, type ActorWithScope } from '@/server/guard';

export const PLATFORM_VIEW = 'platform:customer:view::allow';
export const PLATFORM_MANAGE = 'platform:customer:manage::allow';
export const PLATFORM_SUPPORT = 'platform:support:manage::allow';
export const PLATFORM_RBAC = 'platform:rbac:review::allow';

export function canAccessPlatform(actor: Pick<ActorWithScope, 'permissions'>): boolean {
  return hasPermission(actor, PLATFORM_VIEW)
    || hasPermission(actor, 'admin:system:bypass::allow');
}

export async function platformPageActor(): Promise<ActorWithScope | null> {
  const actor = await loadActor();
  if (!actor || !canAccessPlatform(actor)) return null;
  return actor;
}

export async function platformApiGuard(
  req: Request,
  permission: string = PLATFORM_VIEW,
): Promise<{ actor: ActorWithScope; response: null } | { actor: null; response: Response }> {
  const out = await loadActivePermSession(req);
  if (!out) return { actor: null, response: NextResponse.json({ error: 'unauthorized' }, { status: 401 }) };
  const actor = await loadActor();
  if (!actor) return { actor: null, response: NextResponse.json({ error: 'unauthorized' }, { status: 401 }) };
  const allowed = hasPermission(out.session, permission) || hasPermission(out.session, 'admin:system:bypass::allow');
  if (!allowed) return { actor: null, response: NextResponse.json({ error: 'forbidden' }, { status: 403 }) };
  return { actor, response: null };
}
