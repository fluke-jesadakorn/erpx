import 'server-only';
import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';

/** Retained Account/SSO device *binding* reads only.
 *
 * The saved-account command surface (`read`/`initialize`/`activate`/`reserve`/
 * `complete`/`release`/`remove`), its signed browser cookie, the cross-Holding
 * child-session invalidation and the first SSO adoption of a pre-migration
 * signed parent are native: `account_device_http.rs` over
 * `application/account_auth` plus `adapters/account_revocation_postgres.rs` own
 * every write, `account_sso_postgres.rs` adopts an unbound parent in the
 * authorize unit, and the Account route forwards each request to that runtime.
 * Only the SSO device-eligibility check below still runs inside a retained
 * TypeScript transaction. */

export async function deviceSessionSelected(database: SqlDatabase, sessionId: string, required = false) {
  const row = await database.prepare(`SELECT b.active_session_id FROM account_device_accounts a
    JOIN account_device_browsers b ON b.id=a.browser_id WHERE a.session_id=?`).bind(sessionId).first<{ active_session_id: string | null }>();
  return row ? row.active_session_id === sessionId : !required;
}
