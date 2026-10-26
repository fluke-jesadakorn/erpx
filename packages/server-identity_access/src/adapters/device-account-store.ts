import 'server-only';
import { createHmac, randomBytes, randomUUID, timingSafeEqual } from 'node:crypto';
import type { SqlDatabase, TransactionalSqlDatabase } from '@erpx/server-runtime/public/runtime/types';
import { ApiError } from '@erpx/server-runtime/public/http';
import type { DeviceAccounts } from '@erpx/feature-identity_access/lib/device-accounts';
import { revokeProductSessionFamily } from '@erpx/server-identity_access/adapters/product-session-revocation';
import type {DeviceChildSession} from '@erpx/server-identity_access/adapters/device-account-revocation';

export const deviceBrowserCookie = '__Host-erpx-device';
type Browser = { id: string; active_session_id: string | null; revision: number; pending_nonce: string | null; pending_until: number | null };
type Saved = { session_id: string; handle: string; user_id: string; name: string; email: string; image: string | null };
const unavailable = () => new ApiError(409, 'DEVICE_ACCOUNTS_CHANGED', 'Your saved accounts changed. Review the current accounts and try again.');
const maximum = 5;
export async function liveDeviceParent(database:SqlDatabase,id:string,instant=Date.now()){
 return database.prepare(`SELECT s.id FROM session s JOIN sso_session_bindings b ON b.session_id=s.id
  WHERE s.id=? AND b.audience='account' AND b.environment='development' AND b.parent_session_id IS NULL
  AND s."expiresAt"::timestamptz>?::timestamptz AND b.authenticated_at<=? AND b.absolute_expires_at>?
  AND b.absolute_expires_at<=b.authenticated_at+43200000`).bind(id,new Date(instant).toISOString(),instant,instant).first();
}

/** Authenticity of membership comes from Better Auth's signed device cookies.
 * This signed, host-only cookie only groups those proven sessions. */
export function deviceBrowserCodec(secret: string) {
  const signature = (id: string) => createHmac('sha256', secret).update('erpx-device:').update(id).digest('base64url');
  return {
    encode: (id: string) => `${id}.${signature(id)}`,
    decode(value: string): string | null {
      const [id, mac, extra] = value.split('.');
      if (extra || !/^[a-f0-9-]{36}$/.test(id ?? '') || !/^[A-Za-z0-9_-]{43}$/.test(mac ?? '')) return null;
      return timingSafeEqual(Buffer.from(mac), Buffer.from(signature(id))) ? id : null;
    },
  };
}

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

export function deviceAccountStore(owner: TransactionalSqlDatabase, browserId: string | null, now = Date.now, invalidate?: (sessions:readonly DeviceChildSession[])=>Promise<void>) {
  async function lease(database: SqlDatabase) {
    await database.prepare("SET LOCAL lock_timeout='5s'").run();
    // Match private reads, Provider and command continuations: session first.
    await database.prepare('LOCK TABLE session IN EXCLUSIVE MODE').run();
    await database.prepare('LOCK TABLE account_device_browsers,account_device_accounts IN SHARE ROW EXCLUSIVE MODE').run();
    await database.prepare('LOCK TABLE sso_session_bindings IN SHARE MODE').run();
  }
  async function live(database: SqlDatabase, id: string) {
    return liveDeviceParent(database,id,now());
  }
  async function saved(database: SqlDatabase, id: string): Promise<Saved[]> {
    return (await database.prepare(`SELECT a.session_id,a.handle,s."userId" user_id,u.name,u.email,u.image
      FROM account_device_accounts a JOIN session s ON s.id=a.session_id JOIN "user" u ON u.id=s."userId"
      JOIN sso_session_bindings b ON b.session_id=s.id WHERE a.browser_id=?
      AND b.audience='account' AND b.environment='development' AND b.parent_session_id IS NULL
      AND s."expiresAt"::timestamptz>?::timestamptz AND b.authenticated_at<=? AND b.absolute_expires_at>?
      AND b.absolute_expires_at<=b.authenticated_at+43200000 ORDER BY u.name,a.handle`)
      .bind(id, new Date(now()).toISOString(), now(), now()).all<Saved>()).results;
  }
  async function row(database: SqlDatabase, id = browserId) {
    return id ? database.prepare('SELECT * FROM account_device_browsers WHERE id=?').bind(id).first<Browser>() : null;
  }
  async function snapshot(database: SqlDatabase, browser: Browser | null, proof?:readonly string[]): Promise<DeviceAccounts> {
    return { initialized: !!browser, revision: Number(browser?.revision ?? 0), maximum,
      accounts: browser ? (await saved(database, browser.id)).filter(account=>!proof||proof.includes(account.session_id)).map(account => ({ handle: account.handle, name: account.name,
        email: account.email, image: account.image, current: account.session_id === browser.active_session_id })) : [] };
  }
  async function children(database:SqlDatabase,parents:readonly string[]){
    if(!parents.length)return [];
    return (await database.prepare(`SELECT s.id,s."userId" AS "userId" FROM session s JOIN sso_session_bindings b ON b.session_id=s.id
      WHERE b.environment='development' AND b.parent_session_id IN (${parents.map(()=>'?').join(',')})`).bind(...parents).all<DeviceChildSession>()).results;
  }
  async function ensure(database: SqlDatabase, proof: readonly string[], preferred?:string|null) {
    const valid: string[] = [];
    for (const id of new Set(proof)) if (await live(database, id)) valid.push(id);
    let browser = await row(database);
    if(!browser&&preferred&&valid.includes(preferred)){
      const existing=await database.prepare('SELECT browser_id FROM account_device_accounts WHERE session_id=?').bind(preferred).first<{browser_id:string}>();
      if(existing)browser=await row(database,existing.browser_id);
    }
    if (!browser && valid.length) {
      const membership = await database.prepare(`SELECT browser_id FROM account_device_accounts WHERE session_id IN (${valid.map(() => '?').join(',')}) ORDER BY browser_id LIMIT 1`).bind(...valid).first<{ browser_id: string }>();
      if (membership) browser = await row(database, membership.browser_id);
    }
    if (!browser) {
      const id = randomUUID();
      await database.prepare('INSERT INTO account_device_browsers(id,updated_at) VALUES(?,?)').bind(id, now()).run();
      browser = (await row(database, id))!;
    }
    for (const id of valid) {
      const existing = await database.prepare('SELECT browser_id FROM account_device_accounts WHERE session_id=?').bind(id).first<{ browser_id: string }>();
      // Only server-verified signed device cookies may regroup sessions first
      // adopted through independent SSO exchanges. A posted handle is no proof.
      if (existing && existing.browser_id !== browser.id) {
        const oldChildren=await children(database,[id]);await invalidate?.(oldChildren);
        for(const session of oldChildren)await database.prepare('DELETE FROM session WHERE id=?').bind(session.id).run();
        await database.prepare('UPDATE account_device_accounts SET browser_id=? WHERE session_id=?').bind(browser.id,id).run();
      }
      await database.prepare('INSERT INTO account_device_accounts(session_id,browser_id,handle) VALUES(?,?,?) ON CONFLICT(session_id) DO NOTHING')
        .bind(id, browser.id, randomBytes(24).toString('base64url')).run();
    }
    return browser;
  }
  function check(browser: Browser, revision?: number, nonce?: string) {
    if (revision !== undefined && Number(browser.revision) !== revision) throw unavailable();
    if(nonce&&(browser.pending_nonce!==nonce||Number(browser.pending_until)<=now()))throw unavailable();
    if (browser.pending_nonce && Number(browser.pending_until) > now() && browser.pending_nonce !== nonce)
      throw new ApiError(409, 'ACCOUNT_SIGN_IN_PENDING', 'Another account sign-in is in progress. Check its result before retrying.');
  }
  async function select(database: SqlDatabase, browser: Browser, sessionId: string | null) {
    if (browser.active_session_id === sessionId) return browser;
    // Saved parents stay live. Deleting children makes stale commands and Push
    // fail their existing live-session fences, including A -> B -> A switches.
    if (browser.active_session_id || Number(browser.revision) > 0) {
      const parents=(await database.prepare('SELECT session_id FROM account_device_accounts WHERE browser_id=?').bind(browser.id).all<{session_id:string}>()).results;
      const oldChildren=await children(database,parents.map(parent=>parent.session_id));await invalidate?.(oldChildren);
      for(const session of oldChildren)await database.prepare('DELETE FROM session WHERE id=?').bind(session.id).run();
    }
    await database.prepare('UPDATE account_device_browsers SET active_session_id=?,revision=revision+1,pending_nonce=NULL,pending_until=NULL,updated_at=? WHERE id=?')
      .bind(sessionId, now(), browser.id).run();
    return (await row(database, browser.id))!;
  }
  return {
    read: (proof?:readonly string[]) => owner.transaction(async database => snapshot(database, await row(database),proof)),
    initialize: (proof: readonly string[], active: string | null) => owner.transaction(async database => {
      await lease(database); let browser = await ensure(database, proof,active); check(browser);
      // Initialization cannot overwrite a later selection with a stale cookie.
      if (!browser.active_session_id && Number(browser.revision)===0 && active && proof.includes(active) && await live(database, active)) browser = await select(database, browser, active);
      return { browserId: browser.id, state: await snapshot(database, browser) };
    }),
    activate: (proof: readonly string[], handle: string, revision: number) => owner.transaction(async database => {
      await lease(database); const browser = await ensure(database, proof); check(browser, revision);
      const account = (await saved(database, browser.id)).find(account => account.handle === handle && proof.includes(account.session_id));
      if (!account) throw new ApiError(401, 'DEVICE_ACCOUNT_EXPIRED', 'This saved account expired. Sign in again.');
      const current = await select(database, browser, account.session_id);
      return { browserId: browser.id, sessionId: account.session_id, state: await snapshot(database, current) };
    }),
    reserve: (proof: readonly string[], email: string, revision?: number, active?:string|null) => owner.transaction(async database => {
      await lease(database); let browser = await ensure(database, proof,active);
      if(!browser.active_session_id&&Number(browser.revision)===0&&active&&proof.includes(active)&&await live(database,active))browser=await select(database,browser,active);
      check(browser, revision);
      const accounts = await saved(database, browser.id);
      if (accounts.length >= maximum && !accounts.some(account => account.email.toLowerCase() === email.toLowerCase()))
        throw new ApiError(409, 'DEVICE_ACCOUNT_LIMIT', 'You can save up to five accounts. Remove an account from this device first.');
      const nonce = randomUUID();
      await database.prepare('UPDATE account_device_browsers SET pending_nonce=?,pending_until=?,updated_at=? WHERE id=?').bind(nonce, now() + 120000, now(), browser.id).run();
      return { browserId: browser.id, nonce };
    }),
    complete: (proof: readonly string[], sessionId: string, nonce?: string) => owner.transaction(async database => {
      await lease(database); let browser = await row(database);
      if (browser) check(browser, undefined, nonce);
      if (!await live(database, sessionId) || !proof.includes(sessionId)) throw unavailable();
      const existing = browser ? await saved(database, browser.id) : [];
      const user = await database.prepare('SELECT "userId" user_id FROM session WHERE id=?').bind(sessionId).first<{ user_id: string }>();
      if (existing.length >= maximum && !existing.some(account => account.user_id === user?.user_id)) throw new ApiError(409, 'DEVICE_ACCOUNT_LIMIT', 'Remove an account from this device before adding another.');
      browser = await ensure(database, proof);
      browser = await select(database, browser, sessionId);
      await database.prepare('UPDATE account_device_browsers SET pending_nonce=NULL,pending_until=NULL WHERE id=?').bind(browser.id).run();
      return { browserId: browser.id, state: await snapshot(database, browser) };
    }),
    release: (nonce: string) => owner.prepare('UPDATE account_device_browsers SET pending_nonce=NULL,pending_until=NULL WHERE id=? AND pending_nonce=?').bind(browserId, nonce).run(),
    remove: (proof: readonly string[], handle: string, revision: number) => owner.transaction(async database => {
      await lease(database); let browser = await ensure(database, proof); check(browser, revision);
      const account = (await saved(database, browser.id)).find(account => account.handle === handle && proof.includes(account.session_id));
      if (!account) throw unavailable();
      if (browser.active_session_id === account.session_id) browser = await select(database, browser, null);
      else { await invalidate?.(await children(database,[account.session_id])); await database.prepare('UPDATE account_device_browsers SET revision=revision+1,updated_at=? WHERE id=?').bind(now(), browser.id).run(); browser = (await row(database, browser.id))!; }
      await revokeProductSessionFamily(database, account.session_id, 'development');
      await database.prepare('DELETE FROM account_device_accounts WHERE session_id=? AND browser_id=?').bind(account.session_id, browser.id).run();
      return { browserId: browser.id, sessionId: account.session_id, state: await snapshot(database, browser) };
    }),
  };
}
