import {productNavigationHref} from "@erpx/feature-work_projection/lib/product-navigation";
import { beforeAll, expect, it } from "vitest";
import { env, applyMigrations } from "../../support/runtime-env";
import { productSsoStore, ssoHash } from "@erpx/server-identity_access/adapters/product-sso-store";
import { randomUUID } from "node:crypto";

const db = env.CONTROL_DB;
const origins = { pms:"https://pms-dev-erpx.jesadakorn.com", account: "https://account-dev-erpx.jesadakorn.com", finance: "https://finance-dev-erpx.jesadakorn.com", people: "https://people-dev-erpx.jesadakorn.com" };
let instant = 1_800_000_000_000;
const store = productSsoStore(db, "development", origins, () => instant);
beforeAll(async () => { await applyMigrations(db, env.CONTROL_TEST_MIGRATIONS); });
async function parent(register = true) {
  const id = randomUUID(), user = randomUUID();
  const createdAt = new Date(instant - 60_000), expiresAt = new Date(instant + 3_600_000);
  await db.prepare('INSERT INTO "user"(id,name,email,"emailVerified","createdAt","updatedAt") VALUES(?,?,?,1,?,?)').bind(user, "Fictional employee", `${user}@example.test`, createdAt.getTime(), createdAt.getTime()).run();
  await db.prepare('INSERT INTO session(id,token,"userId","createdAt","updatedAt","expiresAt") VALUES(?,?,?,?,?,?)').bind(id, randomUUID(), user, createdAt.toISOString(), createdAt.toISOString(), expiresAt.toISOString()).run();
  if (register) await store.registerCentral({ id, createdAt, expiresAt });
  return { id, user, createdAt, expiresAt };
}
async function flow(parentId: string, audience = "finance") {
  const browser = await store.start(audience, "/holdings/acme/finance/documents?status=paid&code=old");
  const grant = await store.authorize({ transactionId: browser.id, state: browser.state, challenge: browser.challenge, parentSessionId: parentId });
  return { browser, grant, input: { audience, code: grant.code, state: browser.state, browserCookie: browser.browserCookie } };
}
it("does not upgrade a legacy session by observing its origin", async () => {
  const p = await parent(false);
  expect(await store.centralIsCurrent(p.id)).toBe(false);
  await expect(flow(p.id)).rejects.toThrow("INVALID_SSO_TRANSACTION");
});
it("mints a host-bound child with original authentication time, hashed token and parent expiry", async () => {
  const p = await parent(), f = await flow(p.id);
  const child = await store.consume(f.input);
  const session = await store.session(child.token, "finance");
  expect(session?.session.createdAt).toEqual(p.createdAt);
  expect(session?.session.expiresAt).toEqual(p.expiresAt);
  expect(session?.user.id).toBe(p.user);
  expect(child.returnPath).toBe("/holdings/acme/finance/documents?status=paid");
  expect(await store.session(child.token, "people")).toBeNull();
  expect(await store.session(child.token, "account")).toBeNull();
  expect((await db.prepare('SELECT token,"createdAt" FROM session WHERE id=?').bind(child.sessionId).first<{ token: string; createdAt: string }>())!).toEqual({ token: ssoHash(child.token), createdAt: p.createdAt.toISOString() });
});
it("atomically consumes both browser transaction and authorization grant under concurrency", async () => {
  const p = await parent(), f = await flow(p.id);
  const results = await Promise.allSettled([store.consume(f.input), store.consume(f.input)]);
  expect(results.filter(result => result.status === "fulfilled")).toHaveLength(1);
  expect(results.filter(result => result.status === "rejected")).toHaveLength(1);
  await expect(store.consume(f.input)).rejects.toThrow("INVALID_SSO_TRANSACTION");
  expect((await db.prepare("SELECT count(*) AS total FROM sso_session_bindings WHERE parent_session_id=?").bind(p.id).first<{ total: number }>())!.total).toBe(1);
});
it("rejects altered state, audience, browser secret and PKCE without consuming a valid grant", async () => {
  const p = await parent(), f = await flow(p.id);
  for (const input of [{ ...f.input, state: "wrong" }, { ...f.input, audience: "people" }, { ...f.input, browserCookie: f.input.browserCookie.replace(/\.[^.]+\./, ".wrong.") }, { ...f.input, browserCookie: f.input.browserCookie.split(".").slice(0, 2).join(".") + ".wrong" }]) await expect(store.consume(input)).rejects.toThrow("INVALID_SSO_TRANSACTION");
  expect((await store.consume(f.input)).sessionId).toBeTruthy();
});
it("rejects a grant at the exact 60-second boundary and a browser transaction at five minutes", async () => {
  const p = await parent(), f = await flow(p.id);
  instant += 60_000;
  await expect(store.consume(f.input)).rejects.toThrow("INVALID_SSO_TRANSACTION");
  const browser = await store.start("finance");
  instant += 300_000;
  await expect(store.authorize({ transactionId: browser.id, state: browser.state, challenge: browser.challenge, parentSessionId: p.id })).rejects.toThrow("INVALID_SSO_TRANSACTION");
});
it("invalidates children on parent deletion, password-reset deletion or parent expiry reduction", async () => {
  for (const reason of ["deleted", "password-reset", "expired"] as const) {
    const p = await parent(), f = await flow(p.id), child = await store.consume(f.input);
    if (reason === "deleted") await db.prepare("DELETE FROM session WHERE id=?").bind(p.id).run();
    if (reason === "password-reset") await db.prepare('DELETE FROM session WHERE "userId"=?').bind(p.user).run();
    if (reason === "expired") await db.prepare('UPDATE session SET "expiresAt"=? WHERE id=?').bind(new Date(instant).toISOString(), p.id).run();
    expect(await store.session(child.token, "finance"), reason).toBeNull();
  }
});
it("revokes a login family across apps without revoking an independent login", async () => {
  const p = await parent(), other = await parent();
  const finance = await store.consume((await flow(p.id)).input), people = await store.consume((await flow(p.id, "people")).input);
  const otherFinance = await store.consume((await flow(other.id)).input);
  const waiting = await flow(p.id);
  await store.revokeFamily(people.sessionId);
  expect(await store.session(finance.token, "finance")).toBeNull();
  expect(await store.session(people.token, "people")).toBeNull();
  expect(await store.session(otherFinance.token, "finance")).not.toBeNull();
  await expect(store.consume(waiting.input)).rejects.toThrow("INVALID_SSO_TRANSACTION");
});
it("does not retain a child after family sign-out races its callback", async () => {
  const p = await parent(), f = await flow(p.id);
  const results = await Promise.allSettled([store.consume(f.input), store.revokeFamily(p.id)]);
  expect(results[1].status).toBe("fulfilled");
  if (results[0].status === "fulfilled") expect(await store.session(results[0].value.token, "finance")).toBeNull();
  expect(await store.centralIsCurrent(p.id)).toBe(false);
});
it("reports an earlier parent expiry without refreshing child authentication", async () => {
  const p = await parent(), child = await store.consume((await flow(p.id)).input);
  const earlier = new Date(instant + 1000);
  await db.prepare('UPDATE session SET "expiresAt"=? WHERE id=?').bind(earlier.toISOString(), p.id).run();
  expect((await store.session(child.token, "finance"))?.session.expiresAt).toEqual(earlier);
});
it("rejects a tampered callback and a child masquerading as the central parent", async () => {
  const p = await parent(), f = await flow(p.id);
  await db.prepare("UPDATE sso_authorization_grants SET callback=? WHERE code_hash=?").bind("https://evil.test/sso/callback", ssoHash(f.grant.code)).run();
  await expect(store.consume(f.input)).rejects.toThrow("INVALID_SSO_TRANSACTION");
  const child = await store.consume((await flow(p.id)).input);
  await expect(flow(child.sessionId)).rejects.toThrow("INVALID_SSO_TRANSACTION");
  await expect(store.start("unowned-future-app")).rejects.toThrow("INVALID_SSO_TRANSACTION");
});

it("preserves real selected-Company Office and Front record handoffs through single-use SSO without accepting another audience",async()=>{
 const p=await parent(),scope={audience:'assets' as const,companyId:'company-two',companySlug:'acme-co',workspaceBase:'/holdings/acme'};
 for(const [path,audience,expected] of [['finance/invoices/i1?q=retained#workflow-owners','finance','/holdings/acme/finance/documents/invoices/i1/overview?q=retained#workflow-owners'],['pms/guest-accounts?property=h1#charges','pms','/acme/acme-co/holding/guest-accounts?property=h1#charges']] as const){
  const href=productNavigationHref('/holdings/acme/'+path,scope),launch=new URL(href.href).searchParams.get('next')!,browser=await store.start(audience,launch),grant=await store.authorize({transactionId:browser.id,state:browser.state,challenge:browser.challenge,parentSessionId:p.id}),input={audience,code:grant.code,state:browser.state,browserCookie:browser.browserCookie};
  await expect(store.consume({...input,audience:'people'})).rejects.toThrow('INVALID_SSO_TRANSACTION');const child=await store.consume(input);expect(child.returnPath).toBe(launch);const next=new URL(child.returnPath,'https://launch.invalid');expect(next.searchParams.get('companyId')).toBe('company-two');expect(next.searchParams.get('next')).toBe(expected);expect((await store.session(child.token,audience))?.user.id).toBe(p.user);expect(await store.session(child.token,'people')).toBeNull();await expect(store.consume(input)).rejects.toThrow('INVALID_SSO_TRANSACTION');
 }
});
