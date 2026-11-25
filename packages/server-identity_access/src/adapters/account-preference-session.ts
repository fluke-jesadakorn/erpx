import "server-only";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { ApiError } from "@erpx/server-runtime/public/http";
import { bindingIsCurrent, type SsoSessionBinding } from "@erpx/server-identity_access/domain/product-sso";
import {deviceSessionSelected} from '@erpx/server-identity_access/adapters/device-account-store';
import {planIdentityControlRead,PC_PREF_BINDING_PROBE,PC_PREF_SESSION_SHARE,PC_PREF_SESSION_IMAGE,PC_PREF_BINDING_SHARE,PC_PREF_PARENT_BINDING_SHARE} from './native-control-read-plan';
/** The native owner plans the exact (possibly FOR SHARE) statement; the caller
 * binds and executes it on its own transaction so parent-first lock order and
 * the session authority check remain unchanged. */
async function planned(transaction:SqlDatabase,kind:string,value:string){
 const plan=await planIdentityControlRead(kind,[value]);
 return transaction.prepare(plan.sql).bind(...plan.values);
}

type Binding = { session_id: string; audience: string; environment: string; parent_session_id: string | null; authenticated_at: number; absolute_expires_at: number };
const binding = (row: Binding): SsoSessionBinding => ({ sessionId: row.session_id, audience: row.audience, environment: row.environment,
  parentSessionId: row.parent_session_id, authenticatedAt: Number(row.authenticated_at), absoluteExpiresAt: Number(row.absolute_expires_at) });

/** Called only after normal host-specific authentication. Parent-first locks
 * serialize family deletion with preference writes, without refreshing auth. */
export async function assertAccountPreferenceSession(transaction: SqlDatabase,
  actor: { userId: string; sessionId: string; audience: string | null }, clock = Date.now) {
  const invalid = () => new ApiError(401, "SESSION_REVOKED", "Sign in again to edit your account.");
  const first = await (await planned(transaction,PC_PREF_BINDING_PROBE,actor.sessionId)).first<Binding>();
  if (!actor.audience && first || actor.audience && !first) throw invalid();
  const parentId = first?.parent_session_id;
  for (const id of [parentId, actor.sessionId].filter((id): id is string => !!id))
    await (await planned(transaction,PC_PREF_SESSION_SHARE,id)).first();
  const session = await (await planned(transaction,PC_PREF_SESSION_IMAGE,actor.sessionId)).first<{ user_id: string; expires_at: string }>();
  if (!session || session.user_id !== actor.userId) throw invalid();
  if (!actor.audience) { if (new Date(session.expires_at).getTime() <= clock()) throw invalid(); return; }
  const current = await (await planned(transaction,PC_PREF_BINDING_SHARE,actor.sessionId)).first<Binding>();
  if (!current || current.parent_session_id !== (parentId ?? null)) throw invalid();
  if (actor.audience === "account") {
    const now = clock();
    if (parentId || !bindingIsCurrent(binding(current), "account", "development", now) || new Date(session.expires_at).getTime() <= now || !await deviceSessionSelected(transaction, actor.sessionId)) throw invalid();
    return;
  }
  if (!parentId) throw invalid();
  const parent = await (await planned(transaction,PC_PREF_PARENT_BINDING_SHARE,parentId)).first<Binding & { user_id: string; expires_at: string }>();
  const now = clock();
  if (new Date(session.expires_at).getTime() <= now || !bindingIsCurrent(binding(current), actor.audience, "development", now) ||
    !parent || parent.parent_session_id || parent.user_id !== actor.userId || new Date(parent.expires_at).getTime() <= now ||
    !bindingIsCurrent(binding(parent), "account", "development", now) || Number(parent.authenticated_at) !== Number(current.authenticated_at) ||
    Number(parent.absolute_expires_at) < Number(current.absolute_expires_at) || !await deviceSessionSelected(transaction, parentId)) throw invalid();
}
