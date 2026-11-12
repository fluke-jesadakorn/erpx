import 'server-only';
import { randomBytes, randomUUID } from 'node:crypto';
import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';

/** Retained Account/SSO device *binding* reads only.
 *
 * The saved-account command surface (`read`/`initialize`/`activate`/`reserve`/
 * `complete`/`release`/`remove`), its signed browser cookie and the cross-Holding
 * child-session invalidation are native: `account_device_http.rs` over
 * `application/account_auth` plus `adapters/account_revocation_postgres.rs`
 * own every write, and the Account route forwards each request to that runtime.
 * Only the SSO adoption/eligibility checks below still run inside a retained
 * TypeScript transaction. */

export async function deviceSessionSelected(database: SqlDatabase, sessionId: string, required = false) {
  const row = await database.prepare(`SELECT b.active_session_id FROM account_device_accounts a
    JOIN account_device_browsers b ON b.id=a.browser_id WHERE a.session_id=?`).bind(sessionId).first<{ active_session_id: string | null }>();
  return row ? row.active_session_id === sessionId : !required;
}
/** The first normal SSO exchange adopts a pre-migration signed Account parent.
 * Existing unbound product cookies must re-enter SSO before authorizing work. */
export async function ensureDeviceSessionBinding(database:SqlDatabase,sessionId:string,instant:number){
  if(await database.prepare('SELECT 1 FROM account_device_accounts WHERE session_id=?').bind(sessionId).first())return;
  const id=randomUUID();
  await database.prepare('INSERT INTO account_device_browsers(id,active_session_id,revision,updated_at) VALUES(?,?,1,?)').bind(id,sessionId,instant).run();
  await database.prepare('INSERT INTO account_device_accounts(session_id,browser_id,handle) VALUES(?,?,?)').bind(sessionId,id,randomBytes(24).toString('base64url')).run();
}
