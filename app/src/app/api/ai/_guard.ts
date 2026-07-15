// Shared AI route guard. Use at the top of each /api/ai/* handler.

import { NextResponse } from 'next/server';
import { loadActor } from '@folio-lib/server/guard';

export async function aiGuard(opts: {
  action?: string;
  perm?: string;
  stage?: string;
}): Promise<{ ok: true } | { ok: false; response: Response }> {
  const actor = await loadActor();
  if (!actor) {
    return { ok: false, response: NextResponse.json({ error: 'unauthorized' }, { status: 401 }) };
  }
  if (opts.perm || opts.stage) {
    const { hasPermission } = await import('@folio-lib/perm/server');
    const session = { user: actor, permissions: actor.permissions };
    if (!hasPermission(session, opts.perm || opts.stage!)) {
      return { ok: false, response: NextResponse.json({ error: 'forbidden' }, { status: 403 }) };
    }
  }
  return { ok: true };
}