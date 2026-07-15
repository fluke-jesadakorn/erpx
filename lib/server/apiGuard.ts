// HTTP-style guard helper for Next.js route handlers. Returns either
// `{actor, response: null}` or `{actor, response: Response}`.
//
// Usage:
//   const guard = await apiGuard(req, { perm: 'ai:provider:read::allow' });
//   if (guard.response) return guard.response;
//   ... use guard.actor

import 'server-only';
import { NextResponse } from 'next/server';
import {
  loadActor,
  requireAction,
  type ActorWithScope,
} from './guard';
import type { ActionName } from './sessionToken.types';

export interface ApiGuardOpts {
  action?: ActionName;
  perm?: string;
  stage?: string;
}

export interface ApiGuardOk {
  actor: ActorWithScope;
  response: null;
  override: boolean;
}

export interface ApiGuardFail {
  actor: ActorWithScope | null;
  response: Response;
  override: boolean;
}

export async function apiGuard(_req: Request, opts: ApiGuardOpts = {}): Promise<ApiGuardOk | ApiGuardFail> {
  const actor = await loadActor();
  if (!actor) {
    return { actor: null, response: NextResponse.json({ error: 'unauthorized' }, { status: 401 }), override: false };
  }
  if (opts.action || opts.perm || opts.stage) {
    try {
      const res = await requireAction(actor, opts.action ?? 'access', {
        perm: opts.perm,
        stage: opts.stage,
      });
      return { actor, response: null, override: res.override };
    } catch (e: unknown) {
      const status = (e as { status?: number }).status ?? 403;
      const message = (e as { message?: string }).message || 'forbidden';
      return { actor, response: NextResponse.json({ error: message }, { status }), override: false };
    }
  }
  return { actor, response: null, override: false };
}

export async function apiSlipGuard(_req: Request, key: string): Promise<ApiGuardOk | ApiGuardFail> {
  const actor = await loadActor();
  if (!actor) {
    return { actor: null, response: NextResponse.json({ error: 'unauthorized' }, { status: 401 }), override: false };
  }
  const { slipOwnership } = await import('./guard');
  const ok = await slipOwnership(key, actor);
  if (!ok) {
    return { actor, response: NextResponse.json({ error: 'forbidden' }, { status: 403 }), override: false };
  }
  return { actor, response: null, override: false };
}