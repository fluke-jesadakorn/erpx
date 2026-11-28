import { nativePrepare } from '@erpx/identity-access-server/public/native-identity-write';
import {ssoAudienceForHost} from '@erpx/platform-runtime-ui/lib/product-hosts';
import {requestProductOrigin} from '@erpx/platform-runtime-ui/lib/product-hosts';
import "server-only";
import { configuredProductOrigin, productHostForHost } from "@erpx/platform-runtime-ui/lib/product-hosts";
import { runtimeEnv } from "../../../../../platform/runtime/server/src/public/runtime";
import type { PostgresDatabase } from "@erpx/platform-runtime-server/public/runtime/postgres";
import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
import { ApiError, errorResponse } from "@erpx/platform-runtime-server/public/http";
import { BusinessRuleError } from "@erpx/platform-shared-server/index";
import { createIdentitySessionAuthenticator } from "./native-session";
import { assertAccountPreferenceSession } from "@erpx/identity-access-server/adapters/account-preference-session";
import { accountPreferenceKey } from "@erpx/identity-access-server/adapters/account-preference-key";
import { consoleOrigin } from "@erpx/platform-runtime-ui/lib/erpx-hosts";
import { requestPublicOrigin } from "@erpx/manage-erpx-server/public/public-origin";
import { createAccountPreferencesApplication, createAccountPreferencePatch } from "@erpx/identity-access-server/application/account-preferences";
import { defaultAccountPreferences, type AccountProfile } from "@erpx/identity-access-server/domain/account-preferences";
import { nativeAccountProfile } from "@erpx/identity-access-server/public/account-profile-native";

type ProfileRow = { name: string; email: string; language: "en" | "th" | null; appearance: "system" | "light" | "dark" | null; version: number | null };
/** Supplied owner unit only; defaults are read-only and never create a row. */
export async function readAccountProfile(database: SqlDatabase, userId: string): Promise<AccountProfile | null> {
  const row = await database.prepare(`SELECT u.name,u.email,p.language,p.appearance,p.version FROM "user" u
    LEFT JOIN account_preferences p ON p.user_id=u.id WHERE u.id=?`).bind(userId).first<ProfileRow>();
  return row ? { name: row.name, email: row.email, preferences: row.version === null ? { ...defaultAccountPreferences }
    : { language: row.language!, appearance: row.appearance!, version: Number(row.version) } } : null;
}

function commandPorts(owner: PostgresDatabase, actor: { userId: string; sessionId: string; audience: string | null }): Parameters<typeof createAccountPreferencesApplication>[0] {
  return { accountKey: accountPreferenceKey(actor.userId),
    transaction: run => owner.transaction(async transaction => {
      const authorize = () => assertAccountPreferenceSession(transaction, actor);
      return run({
        authorize,
        current: async () => {
          await transaction.prepare('SELECT id FROM "user" WHERE id=? FOR UPDATE').bind(actor.userId).first();
          return readAccountProfile(transaction, actor.userId);
        },
        save: async input => {
          const instant = Date.now();
          const row = input.expectedVersion === 0
            ? await nativePrepare("w_884158937ec5")
              .bind(actor.userId, input.language, input.appearance, instant).first()
            : await nativePrepare("w_3317045d0c3e")
              .bind(input.language, input.appearance, instant, actor.userId, input.expectedVersion).first();
          if (!row) throw new ApiError(409, "ACCOUNT_PREFERENCES_CHANGED", "Your account settings changed. Reload and review the current settings.");
          if (input.name !== input.expectedName) await nativePrepare("account_user_name_update")
            .bind(input.name, instant, actor.userId).run();
          return (await readAccountProfile(transaction, actor.userId))!;
        },
        flush: async () => { await transaction.prepare("SET CONSTRAINTS ALL IMMEDIATE").run(); },
      });
    }),
  };
}

export async function accountProfileRequest(request: Request) {
  const responseHeaders = { "Cache-Control": "private, no-store", "Referrer-Policy": "no-referrer", "X-Content-Type-Options": "nosniff" };
  try {
    const origin = configuredProductOrigin("account");
    if (!origin || productHostForHost(request.headers.get("host")) !== "account") throw new ApiError(404, "NOT_FOUND", "Not found.");
    if (!["GET", "PATCH"].includes(request.method)) return new Response("Method not allowed", { status: 405, headers: { ...responseHeaders, Allow: "GET, PATCH" } });
    if (request.method === "PATCH" && request.headers.get("origin") !== origin) throw new ApiError(403, "ORIGIN_FORBIDDEN", "Use ERPX Account to change your settings.");
    const session = await createIdentitySessionAuthenticator()(requestPublicOrigin(request), request.headers);
    if (!session) throw new ApiError(401, "SIGN_IN_REQUIRED", "Sign in to edit your account.");
    const expectedKey = new URL(request.url).searchParams.get("accountKey");
    if (expectedKey !== null && expectedKey !== accountPreferenceKey(session.user.id))
      throw new ApiError(409, "ACCOUNT_CONTEXT_CHANGED", "Your account changed. Reload before editing these settings.");
    const owner = (await runtimeEnv()).CONTROL_DB as PostgresDatabase;
    if (request.method === "GET") {
      const profile = await nativeAccountProfile(session.user.id);
      return Response.json({ data: profile }, { headers: responseHeaders });
    }
    if (!request.headers.get("content-type")?.startsWith("application/json")) throw new ApiError(415, "JSON_REQUIRED", "Submit your account settings as JSON.");
    const body = await request.text();
    if (new TextEncoder().encode(body).length > 8192) throw new ApiError(413, "REQUEST_TOO_LARGE", "This request is too large.");
    let input: unknown;
    try { input = JSON.parse(body); } catch { throw new ApiError(422, "INVALID_JSON", "Submit valid account settings."); }
    const result = await createAccountPreferencesApplication(commandPorts(owner, { userId: session.user.id, sessionId: session.session.id, audience: "account" }))(input);
    return Response.json({ data: result }, { headers: responseHeaders });
  } catch (failure) {
    const error = failure instanceof BusinessRuleError ? new ApiError(failure.status, failure.code, failure.message) : failure;
    const response = errorResponse(error);
    for (const [key, value] of Object.entries(responseHeaders)) response.headers.set(key, value);
    return response;
  }
}

/** Personal preferences may be changed from the current owned host. Every
 * mutation stays on that host and carries the rendered account/version. */
export async function accountPreferencesRequest(request: Request) {
  const headers = { "Cache-Control": "private, no-store", "Referrer-Policy": "no-referrer", "X-Content-Type-Options": "nosniff" };
  try {
    const env = await runtimeEnv(), audience = productHostForHost(request.headers.get("host"));
    const origin = requestPublicOrigin(request);
    const known = [consoleOrigin()?.origin, new URL(env.BETTER_AUTH_URL).origin,
      ...[process.env.ERPX_FRONT_POS_ORIGIN, process.env.ERPX_FRONT_PMS_ORIGIN].filter((value): value is string => !!value).map(value => new URL(value).origin)];
    if (!(audience ? requestProductOrigin(audience,request.headers) === origin : known.includes(origin)) || new URL(origin).host !== request.headers.get("host"))
      throw new ApiError(404, "NOT_FOUND", "Not found.");
    if (!["GET", "PATCH"].includes(request.method)) return new Response("Method not allowed", { status: 405, headers: { ...headers, Allow: "GET, PATCH" } });
    if (request.method === "PATCH" && request.headers.get("origin") !== origin)
      throw new ApiError(403, "ORIGIN_FORBIDDEN", "Change these settings from the current application.");
    const session = await createIdentitySessionAuthenticator()(requestPublicOrigin(request), request.headers);
    if (!session) throw new ApiError(401, "SIGN_IN_REQUIRED", "Sign in to save your preferences.");
    const owner = env.CONTROL_DB as PostgresDatabase, key = accountPreferenceKey(session.user.id);
    if (request.method === "GET") {
      const profile = await nativeAccountProfile(session.user.id);
      return Response.json({ data: { key, preference: profile.preferences } }, { headers });
    }
    if (!request.headers.get("content-type")?.startsWith("application/json")) throw new ApiError(415, "JSON_REQUIRED", "Submit valid preferences.");
    const body = await request.text();
    if (new TextEncoder().encode(body).length > 2048) throw new ApiError(413, "REQUEST_TOO_LARGE", "This request is too large.");
    let input: unknown;
    try { input = JSON.parse(body); } catch { throw new ApiError(422, "INVALID_JSON", "Submit valid preferences."); }
    const preference = await createAccountPreferencePatch(commandPorts(owner, { userId: session.user.id, sessionId: session.session.id, audience:ssoAudienceForHost(request.headers.get('host')) }))(input);
    return Response.json({ data: { key, preference } }, { headers });
  } catch (failure) {
    const error = failure instanceof BusinessRuleError ? new ApiError(failure.status, failure.code, failure.message) : failure;
    const response = errorResponse(error);
    for (const [key, value] of Object.entries(headers)) response.headers.set(key, value);
    return response;
  }
}
