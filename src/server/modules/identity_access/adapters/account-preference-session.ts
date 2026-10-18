import "server-only";
import type { SqlDatabase } from "../../runtime/public/runtime/types";
import { ApiError } from "../../runtime/public/http";
import { bindingIsCurrent, type SsoSessionBinding } from "../domain/product-sso";

type Binding = { session_id: string; audience: string; environment: string; parent_session_id: string | null; authenticated_at: number; absolute_expires_at: number };
const binding = (row: Binding): SsoSessionBinding => ({ sessionId: row.session_id, audience: row.audience, environment: row.environment,
  parentSessionId: row.parent_session_id, authenticatedAt: Number(row.authenticated_at), absoluteExpiresAt: Number(row.absolute_expires_at) });

/** Called only after normal host-specific authentication. Parent-first locks
 * serialize family deletion with preference writes, without refreshing auth. */
export async function assertAccountPreferenceSession(transaction: SqlDatabase,
  actor: { userId: string; sessionId: string; audience: string | null }, clock = Date.now) {
  const invalid = () => new ApiError(401, "SESSION_REVOKED", "Sign in again to edit your account.");
  const first = await transaction.prepare("SELECT * FROM sso_session_bindings WHERE session_id=?")
    .bind(actor.sessionId).first<Binding>();
  if (!actor.audience && first || actor.audience && !first) throw invalid();
  const parentId = first?.parent_session_id;
  for (const id of [parentId, actor.sessionId].filter((id): id is string => !!id))
    await transaction.prepare("SELECT id FROM session WHERE id=? FOR SHARE").bind(id).first();
  const session = await transaction.prepare('SELECT "userId" user_id,"expiresAt" expires_at FROM session WHERE id=?')
    .bind(actor.sessionId).first<{ user_id: string; expires_at: string }>();
  if (!session || session.user_id !== actor.userId) throw invalid();
  if (!actor.audience) { if (new Date(session.expires_at).getTime() <= clock()) throw invalid(); return; }
  const current = await transaction.prepare("SELECT * FROM sso_session_bindings WHERE session_id=? FOR SHARE")
    .bind(actor.sessionId).first<Binding>();
  if (!current || current.parent_session_id !== (parentId ?? null)) throw invalid();
  if (actor.audience === "account") {
    const now = clock();
    if (parentId || !bindingIsCurrent(binding(current), "account", "development", now) || new Date(session.expires_at).getTime() <= now) throw invalid();
    return;
  }
  if (!parentId) throw invalid();
  const parent = await transaction.prepare(`SELECT b.*,s."userId" user_id,s."expiresAt" expires_at
    FROM sso_session_bindings b JOIN session s ON s.id=b.session_id WHERE b.session_id=? FOR SHARE OF b`)
    .bind(parentId).first<Binding & { user_id: string; expires_at: string }>();
  const now = clock();
  if (new Date(session.expires_at).getTime() <= now || !bindingIsCurrent(binding(current), actor.audience, "development", now) ||
    !parent || parent.parent_session_id || parent.user_id !== actor.userId || new Date(parent.expires_at).getTime() <= now ||
    !bindingIsCurrent(binding(parent), "account", "development", now) || Number(parent.authenticated_at) !== Number(current.authenticated_at) ||
    Number(parent.absolute_expires_at) < Number(current.absolute_expires_at)) throw invalid();
}
