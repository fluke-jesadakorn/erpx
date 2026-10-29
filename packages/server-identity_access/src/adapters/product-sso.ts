import "server-only";
import { NextResponse } from "next/server";
import { configuredProductOrigin, ssoAudienceForHost, trustedProductOrigin } from "@erpx/feature-runtime/lib/product-hosts";
import { runtimeEnv } from "../../../../packages/server-runtime/src/public/runtime";
import type { PostgresDatabase } from "@erpx/server-runtime/public/runtime/postgres";
import { productSsoStore } from "@erpx/server-identity_access/adapters/product-sso-store";

const sessionCookieName = "__Host-erpx-product";
const browserCookieName = "__Host-erpx-sso";
const cookieOptions = { secure: true, httpOnly: true, sameSite: "lax" as const, path: "/" };
const responseHeaders = { "Cache-Control": "private, no-store", "Referrer-Policy": "no-referrer", "X-Content-Type-Options": "nosniff" };
import { cookie, ssoOrigins } from "@erpx/server-identity_access/adapters/product-sso-context";
export { productSessionCookie, ssoOrigins } from "@erpx/server-identity_access/adapters/product-sso-context";
export async function runtimeProductSsoStore() {
  const env = await runtimeEnv();
  return productSsoStore(env.CONTROL_DB as PostgresDatabase, "development", ssoOrigins());
}
function destinationResponse(destination: URL) { return NextResponse.redirect(destination, { status: 303, headers: responseHeaders }); }
function host(request: Request) { return ssoAudienceForHost(request.headers.get("host")); }

export async function productSsoRequest(request: Request, action: string) {
  const audience = host(request), accountOrigin = configuredProductOrigin("account");
  if (!audience || !accountOrigin) return new Response("Not found", { status: 404, headers: responseHeaders });
  const url = new URL(request.url), origin = ssoOrigins()[audience];
  if (request.method !== "GET") return new Response("Method not allowed", { status: 405, headers: { ...responseHeaders, Allow: "GET" } });
  const store = await runtimeProductSsoStore();
  try {
    if (action === "start" && audience !== "account") {
      const transaction = await store.start(audience, url.searchParams.get("next"));
      const authorize = new URL("/sso/authorize", accountOrigin);
      authorize.search = new URLSearchParams({ transaction: transaction.id, state: transaction.state, challenge: transaction.challenge }).toString();
      const response = destinationResponse(authorize);
      response.cookies.set(browserCookieName, transaction.browserCookie, { ...cookieOptions, maxAge: 300 });
      return response;
    }
    if (action === "authorize" && audience === "account") {
      const { createAuth } = await import("@erpx/server-identity_access/adapters/auth");
      const session = await (await createAuth(accountOrigin)).api.getSession({ headers: request.headers });
      if (!session) {
        // Account login carries an opaque transaction id; it never carries a session token.
        const login = new URL("/login", accountOrigin);
        login.searchParams.set("sso", url.searchParams.get("transaction") ?? "");
        login.searchParams.set("state", url.searchParams.get("state") ?? "");
        login.searchParams.set("challenge", url.searchParams.get("challenge") ?? "");
        return destinationResponse(login);
      }
      const grant = await store.authorize({ transactionId: url.searchParams.get("transaction") ?? "", state: url.searchParams.get("state") ?? "", challenge: url.searchParams.get("challenge") ?? "", parentSessionId: session.session.id });
      const callback = new URL(grant.callback);
      callback.search = new URLSearchParams({ code: grant.code, state: url.searchParams.get("state") ?? "" }).toString();
      return destinationResponse(callback);
    }
    if (action === "callback" && audience !== "account") {
      const session = await store.consume({ audience, code: url.searchParams.get("code") ?? "", state: url.searchParams.get("state") ?? "", browserCookie: cookie(request.headers, browserCookieName) });
      const response = destinationResponse(new URL(session.returnPath, origin));
      response.cookies.set(sessionCookieName, session.token, { ...cookieOptions, expires: new Date(session.expiresAt) });
      response.cookies.set(browserCookieName, "", { ...cookieOptions, maxAge: 0 });
      return response;
    }
    return new Response("Not found", { status: 404, headers: responseHeaders });
  } catch (error) {
    if (!(error instanceof Error) || error.message !== "INVALID_SSO_TRANSACTION") throw error;
    // Leave expired credentials out of the recovery URL. Retry starts a new transaction.
    const recovery = new URL("/login?reason=sso_expired", accountOrigin);
    if (audience !== "account") recovery.searchParams.set("app", audience);
    const response = destinationResponse(recovery);
    response.cookies.set(browserCookieName, "", { ...cookieOptions, maxAge: 0 });
    return response;
  }
}

export async function productSignOut(request: Request) {
  const audience = host(request);
  if (!audience) return null;
  const origin = trustedProductOrigin(`https://${request.headers.get('host')}`)!;
  if (request.method !== "POST" || request.headers.get("origin") !== origin) return new Response("Invalid origin", { status: 403, headers: responseHeaders });
  const { createAuth } = await import("@erpx/server-identity_access/adapters/auth");
  const session = await (await createAuth(origin)).api.getSession({ headers: request.headers });
  if (session) await (await runtimeProductSsoStore()).revokeFamily(session.session.id);
  const response = NextResponse.json({ success: true }, { headers: responseHeaders });
  response.cookies.set(sessionCookieName, "", { ...cookieOptions, maxAge: 0 });
  return response;
}
