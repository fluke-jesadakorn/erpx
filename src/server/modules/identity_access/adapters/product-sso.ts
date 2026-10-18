import "server-only";
import { NextResponse } from "next/server";
import { configuredProductOrigin, productHostForHost } from "@/lib/runtime/product-hosts";
import { productIds, sharedHostKeys, type ProductHostKey } from "@/server/modules/commercial/public/product-registry";
import { runtimeEnv } from "@/server/modules/runtime/public/runtime";
import type { PostgresDatabase } from "@/server/modules/runtime/public/runtime/postgres";
import { productSsoStore } from "./product-sso-store";

const sessionCookieName = "__Host-erpx-product";
const browserCookieName = "__Host-erpx-sso";
const cookieOptions = { secure: true, httpOnly: true, sameSite: "lax" as const, path: "/" };
const responseHeaders = { "Cache-Control": "private, no-store", "Referrer-Policy": "no-referrer", "X-Content-Type-Options": "nosniff" };
function cookie(headers: Headers, name: string) {
  const values = headers.get("cookie")?.split(";").map(entry => entry.trim()).filter(entry => entry.startsWith(name + "=")) ?? [];
  if (values.length !== 1) return "";
  try { return decodeURIComponent(values[0].slice(name.length + 1)); } catch { return ""; }
}
export function productSessionCookie(headers: Headers) { return cookie(headers, sessionCookieName); }
export function ssoOrigins() {
  return Object.fromEntries([...productIds, ...sharedHostKeys].flatMap(key => {
    const origin = configuredProductOrigin(key);
    return origin ? [[key, origin]] : [];
  }));
}
export async function runtimeProductSsoStore() {
  const env = await runtimeEnv();
  return productSsoStore(env.CONTROL_DB as PostgresDatabase, "development", ssoOrigins());
}
function destinationResponse(destination: URL) { return NextResponse.redirect(destination, { status: 303, headers: responseHeaders }); }
function host(request: Request): ProductHostKey | null { return productHostForHost(request.headers.get("host")); }

export async function productSsoRequest(request: Request, action: string) {
  const audience = host(request), accountOrigin = configuredProductOrigin("account");
  if (!audience || !accountOrigin) return new Response("Not found", { status: 404, headers: responseHeaders });
  const url = new URL(request.url), origin = configuredProductOrigin(audience)!;
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
      const { createAuth } = await import("./auth");
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
    const response = new NextResponse("Sign-in expired or could not be verified. Start sign-in again from your app.", { status: 400, headers: responseHeaders });
    response.cookies.set(browserCookieName, "", { ...cookieOptions, maxAge: 0 });
    return response;
  }
}

export async function productSignOut(request: Request) {
  const audience = host(request);
  if (!audience) return null;
  const origin = configuredProductOrigin(audience)!;
  if (request.method !== "POST" || request.headers.get("origin") !== origin) return new Response("Invalid origin", { status: 403, headers: responseHeaders });
  const { createAuth } = await import("./auth");
  const session = await (await createAuth(origin)).api.getSession({ headers: request.headers });
  if (session) await (await runtimeProductSsoStore()).revokeFamily(session.session.id);
  const response = NextResponse.json({ success: true }, { headers: responseHeaders });
  response.cookies.set(sessionCookieName, "", { ...cookieOptions, maxAge: 0 });
  return response;
}
