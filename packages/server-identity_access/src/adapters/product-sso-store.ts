import "server-only";
import { createHash, randomBytes, randomUUID, timingSafeEqual } from "node:crypto";
import type { TransactionalSqlDatabase, SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { bindingIsCurrent, browserTransactionLifetime, safeProductReturnPath, sessionLifetime, singleUseGrantLifetime, type SsoSessionBinding } from "@erpx/server-identity_access/domain/product-sso";
import {revokeProductSessionFamily} from '@erpx/server-identity_access/adapters/product-session-revocation';

export const ssoHash = (value: string) => createHash("sha256").update(value).digest("base64url");
const secret = () => randomBytes(32).toString("base64url");
function equal(left: string, right: string) {
  const a = Buffer.from(left), b = Buffer.from(right);
  return a.length === b.length && timingSafeEqual(a, b);
}
function rejected(): never { throw new Error("INVALID_SSO_TRANSACTION"); }
type Transaction = { id: string; browser_hash: string; state_hash: string; challenge: string; audience: string; environment: string; callback: string; return_path: string; created_at: number; expires_at: number; consumed_at: number | null };
type Grant = { code_hash: string; transaction_id: string; parent_session_id: string; audience: string; environment: string; callback: string; state_hash: string; challenge: string; created_at: number; expires_at: number; consumed_at: number | null };
type BindingRow = { session_id: string; audience: string; environment: string; parent_session_id: string | null; authenticated_at: number; absolute_expires_at: number };
const binding = (row: BindingRow): SsoSessionBinding => ({ sessionId: row.session_id, audience: row.audience, environment: row.environment, parentSessionId: row.parent_session_id, authenticatedAt: Number(row.authenticated_at), absoluteExpiresAt: Number(row.absolute_expires_at) });

/** All consumed grants, browser transactions and child sessions commit together. */
export function productSsoStore(db: TransactionalSqlDatabase, environment: "development", origins: Readonly<Record<string, string>>, now = Date.now) {
  function callback(audience: string) {
    if (audience === "account" || !Object.hasOwn(origins, audience)) return rejected();
    return new URL("/sso/callback", origins[audience]).href;
  }
  async function central(database: SqlDatabase, id: string, instant: number, lock = false) {
    const row = await database.prepare(`SELECT b.*,s."userId" AS user_id,s."expiresAt" AS session_expires_at
      FROM sso_session_bindings b JOIN session s ON s.id=b.session_id
      WHERE b.session_id=? ${lock ? "FOR UPDATE OF s" : ""}`).bind(id).first<BindingRow & { user_id: string; session_expires_at: string }>();
    if (!row || row.parent_session_id || !bindingIsCurrent(binding(row), "account", environment, instant) || new Date(row.session_expires_at).getTime() <= instant) return rejected();
    return row;
  }
  return {
    /** Called only for a newly created account-host credential/directory session. */
    async registerCentral(session: { id: string; createdAt: Date; expiresAt: Date }) {
      const authenticatedAt = session.createdAt.getTime();
      await db.prepare(`INSERT INTO sso_session_bindings(session_id,audience,environment,parent_session_id,authenticated_at,absolute_expires_at)
        VALUES(?,'account',?,NULL,?,?)`).bind(session.id, environment, authenticatedAt, Math.min(session.expiresAt.getTime(), authenticatedAt + sessionLifetime)).run();
    },
    async centralIsCurrent(sessionId: string) {
      try { await central(db, sessionId, now()); return true; } catch (error) {
        if (error instanceof Error && error.message === "INVALID_SSO_TRANSACTION") return false;
        throw error;
      }
    },
    async start(audience: string, returnPath?: string | null) {
      const instant = now(), id = randomUUID(), browser = secret(), verifier = secret(), state = secret();
      const redirectUri = callback(audience), challenge = ssoHash(verifier);
      await db.prepare(`INSERT INTO sso_browser_transactions(id,browser_hash,state_hash,challenge,audience,environment,callback,return_path,created_at,expires_at)
        VALUES(?,?,?,?,?,?,?,?,?,?)`).bind(id, ssoHash(browser), ssoHash(state), challenge, audience, environment, redirectUri, safeProductReturnPath(returnPath), instant, instant + browserTransactionLifetime).run();
      return { id, state, challenge, browserCookie: `${id}.${browser}.${verifier}`, callback: redirectUri };
    },
    async authorize(input: { transactionId: string; state: string; challenge: string; parentSessionId: string }) {
      const instant = now(), code = secret();
      return db.transaction(async tx => {
        const browser = await tx.prepare("SELECT * FROM sso_browser_transactions WHERE id=? FOR UPDATE").bind(input.transactionId).first<Transaction>();
        if (!browser || browser.environment !== environment || browser.expires_at <= instant || browser.consumed_at !== null ||
          browser.callback !== callback(browser.audience) || !equal(browser.state_hash, ssoHash(input.state)) || !equal(browser.challenge, input.challenge)) return rejected();
        const parent = await central(tx, input.parentSessionId, instant, true);
        const expires = Math.min(instant + singleUseGrantLifetime, Number(browser.expires_at), Number(parent.absolute_expires_at), new Date(parent.session_expires_at).getTime());
        if (expires <= instant) return rejected();
        await tx.prepare(`INSERT INTO sso_authorization_grants(code_hash,transaction_id,parent_session_id,audience,environment,callback,state_hash,challenge,created_at,expires_at)
          VALUES(?,?,?,?,?,?,?,?,?,?)`).bind(ssoHash(code), browser.id, parent.session_id, browser.audience, environment, browser.callback, browser.state_hash, browser.challenge, instant, expires).run();
        return { code, callback: browser.callback };
      });
    },
    async consume(input: { audience: string; code: string; state: string; browserCookie: string }) {
      const parts = input.browserCookie.split(".");
      if (parts.length !== 3 || parts.some(part => !part || part.length > 128) || input.code.length > 128 || input.state.length > 128) return rejected();
      const [id, browserSecret, verifier] = parts;
      const instant = now(), token = secret(), sessionId = randomUUID();
      return db.transaction(async tx => {
        const browser = await tx.prepare("SELECT * FROM sso_browser_transactions WHERE id=? FOR UPDATE").bind(id).first<Transaction>();
        const grant = await tx.prepare("SELECT * FROM sso_authorization_grants WHERE code_hash=? FOR UPDATE").bind(ssoHash(input.code)).first<Grant>();
        if (!browser || !grant || browser.consumed_at !== null || grant.consumed_at !== null || browser.expires_at <= instant || grant.expires_at <= instant ||
          browser.environment !== environment || grant.environment !== environment || browser.audience !== input.audience || grant.audience !== input.audience ||
          grant.transaction_id !== browser.id || grant.callback !== callback(input.audience) || browser.callback !== grant.callback ||
          !equal(browser.browser_hash, ssoHash(browserSecret)) || !equal(browser.state_hash, ssoHash(input.state)) || !equal(grant.state_hash, browser.state_hash) ||
          !equal(browser.challenge, ssoHash(verifier)) || !equal(grant.challenge, browser.challenge)) return rejected();
        const parent = await central(tx, grant.parent_session_id, instant, true);
        const expiresAt = Math.min(Number(parent.absolute_expires_at), new Date(parent.session_expires_at).getTime());
        const date = new Date(instant).toISOString();
        await tx.prepare(`INSERT INTO session(id,token,"userId","createdAt","updatedAt","expiresAt") VALUES(?,?,?,?,?,?)`)
          .bind(sessionId, ssoHash(token), parent.user_id, new Date(Number(parent.authenticated_at)).toISOString(), date, new Date(expiresAt).toISOString()).run();
        await tx.prepare(`INSERT INTO sso_session_bindings(session_id,audience,environment,parent_session_id,authenticated_at,absolute_expires_at) VALUES(?,?,?,?,?,?)`)
          .bind(sessionId, input.audience, environment, parent.session_id, parent.authenticated_at, expiresAt).run();
        await tx.prepare("UPDATE sso_browser_transactions SET consumed_at=? WHERE id=?").bind(instant, browser.id).run();
        await tx.prepare("UPDATE sso_authorization_grants SET consumed_at=? WHERE code_hash=?").bind(instant, grant.code_hash).run();
        return { token, sessionId, expiresAt, returnPath: browser.return_path };
      });
    },
    async session(token: string, audience: string) {
      if (!/^[A-Za-z0-9_-]{43}$/.test(token) || !Object.hasOwn(origins, audience) || audience === "account") return null;
      const instant = now();
      const row = await db.prepare(`SELECT s.id,s."expiresAt",s."userId",b.*,u.name,u.email,u."emailVerified",u.image,u."createdAt" AS user_created_at,u."updatedAt" AS user_updated_at,
        p."expiresAt" AS parent_expires_at,p."userId" AS parent_user_id,pb.audience AS parent_audience,pb.environment AS parent_environment,pb.parent_session_id AS grandparent,
        pb.absolute_expires_at AS parent_absolute_expires_at,pb.authenticated_at AS parent_authenticated_at
        FROM session s JOIN sso_session_bindings b ON b.session_id=s.id JOIN "user" u ON u.id=s."userId"
        JOIN session p ON p.id=b.parent_session_id JOIN sso_session_bindings pb ON pb.session_id=p.id
        WHERE s.token=?`).bind(ssoHash(token)).first<BindingRow & {
          id: string; expiresAt: string; userId: string; name: string; email: string; emailVerified: number; image: string | null; user_created_at: number; user_updated_at: number;
          parent_expires_at: string; parent_user_id: string; parent_audience: string; parent_environment: string; grandparent: string | null; parent_absolute_expires_at: number; parent_authenticated_at: number;
        }>();
      if (!row || !bindingIsCurrent(binding(row), audience, environment, instant) || row.parent_audience !== "account" || row.parent_environment !== environment || row.grandparent ||
        row.parent_user_id !== row.userId || Number(row.parent_authenticated_at) !== Number(row.authenticated_at) || Number(row.parent_absolute_expires_at) < Number(row.absolute_expires_at) ||
        new Date(row.parent_expires_at).getTime() <= instant || new Date(row.expiresAt).getTime() <= instant) return null;
      return {
        session: { id: row.id, userId: row.userId, token: "", expiresAt: new Date(Math.min(new Date(row.expiresAt).getTime(), new Date(row.parent_expires_at).getTime(), Number(row.absolute_expires_at))), createdAt: new Date(Number(row.authenticated_at)), updatedAt: new Date(Number(row.authenticated_at)), ipAddress: null, userAgent: null },
        user: { id: row.userId, name: row.name, email: row.email, emailVerified: Boolean(Number(row.emailVerified)), image: row.image, createdAt: new Date(Number(row.user_created_at)), updatedAt: new Date(Number(row.user_updated_at)) },
      };
    },
    async revokeFamily(sessionId: string) {
      await db.transaction(tx=>revokeProductSessionFamily(tx,sessionId,environment));
    },
  };
}
