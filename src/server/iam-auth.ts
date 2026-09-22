import { iamThrottle } from "./iam-throttle";
import "server-only";
import { z } from "zod";
import { decodeJwt } from "jose";
import { createAuthEndpoint, APIError } from "better-auth/api";
import { setSessionCookie } from "better-auth/cookies";
import type { GenericOAuthConfig } from "better-auth/plugins/generic-oauth";


import { iamDb } from "./iam-db";
import { runtimeEnv } from "./runtime";

async function directoryUser(connectionId: string, externalId: string) {
  const { routedConnection, synchronizeConnection } = await import("./iam-directories");
  const { db, connection } = await routedConnection(connectionId);
  if (connection.status !== "active")
    throw new APIError("FORBIDDEN", {
      message: "Directory sign-in is unavailable.",
    });
  await synchronizeConnection(db, connection);
  const identity = await db
    .prepare(
      `SELECT i.user_id FROM iam_identities i JOIN iam_members m ON m.identity_id=i.id AND m.company_id=i.company_id AND m.user_id=i.user_id WHERE i.connection_id=? AND i.external_id=? AND i.enabled=true AND i.observed_at>? AND m.status='active' AND m.login_policy='directory'`,
    )
    .bind(connectionId, externalId, Date.now() - 300_000)
    .first<{ user_id: string }>();
  if (!identity)
    throw new APIError("FORBIDDEN", {
      message: "Your directory identity has not been approved for ERPX access.",
    });
  return identity.user_id;
}
export function directoryAuthPlugin() {
  return {
    id: "erpx-directory",
    endpoints: {
      directorySignIn: createAuthEndpoint(
        "/directory/sign-in",
        {
          method: "POST",
          body: z.object({
            connectionId: z.string().uuid(),
            username: z.string().min(1).max(200),
            password: z.string().min(1).max(1024),
          }),
        },
        async (ctx) => {
          await iamThrottle(
            "directory",
            ctx.body.connectionId + ":" + ctx.body.username,
          );
          try {
            const { routedConnection } = await import("./iam-directories");
            const { verifyLdapPassword } = await import("./iam-providers");
            const { connection } = await routedConnection(
              ctx.body.connectionId,
            );
            if (
              connection.status !== "active" ||
              connection.config.mode !== "ldaps"
            )
              throw new Error("Directory unavailable");
            const externalId = await verifyLdapPassword(
                connection,
                ctx.body.username,
                ctx.body.password,
              ),
              userId = await directoryUser(connection.id, externalId);
            const user = await ctx.context.internalAdapter.findUserById(userId);
            if (!user) throw new Error("Account unavailable");
            const session = await ctx.context.internalAdapter.createSession(
              user.id,
            );
            if (!session) throw new Error("Session unavailable");
            await setSessionCookie(ctx, { session, user });
            return ctx.json({ success: true });
          } catch {
            throw new APIError("UNAUTHORIZED", {
              message:
                "Directory sign-in failed. Check your credentials and approved access.",
            });
          }
        },
      ),
    },
  };
}
/** Register only the explicitly selected provider. An unavailable corporate issuer
 * must never prevent local sign-in, emergency recovery, or ordinary session reads. */
export async function directoryOidcConfig(
  id: string,
): Promise<GenericOAuthConfig> {
  const { routedConnection } = await import("./iam-directories");
  const { directorySecret } = await import("./iam-providers");
  const { connection } = await routedConnection(id);
  if (connection.status !== "active" || connection.config.mode !== "oidc")
    throw new APIError("FORBIDDEN", {
      message: "This sign-in provider is unavailable.",
    });
  const secret = await directorySecret(connection.config, connection.id),
    issuer = connection.config.issuer!;
  return {
    providerId: `directory-${id}`,
    name: connection.name,
    clientId: connection.config.clientId!,
    clientSecret: secret.clientSecret,
    discoveryUrl: `${issuer.replace(/\/$/, "")}/.well-known/openid-configuration`,
    accountIssuer: issuer,
    requireIdTokenVerification: true,
    pkce: true,
    scopes: ["openid", "profile", "email"],
    disableSignUp: true,
    disableImplicitSignUp: true,
    accountSubject: ({ profile }) =>
      String(
        connection.provider === "entra_id"
          ? (profile.oid ?? "")
          : (profile.entryUUID ?? ""),
      ),
    getUserInfo: async (tokens) => {
      if (!tokens.idToken) return null;
      const claims = decodeJwt(tokens.idToken);
      if (
        claims.iss !== issuer ||
        (connection.provider === "entra_id" &&
          String(claims.tid).toLowerCase() !==
            connection.config.tenantId?.toLowerCase())
      )
        return null;
      const externalId =
        connection.provider === "entra_id" ? claims.oid : claims.entryUUID;
      if (typeof externalId !== "string" || !externalId) return null;
      const { db } = await routedConnection(id);
      if (!claims.sub) return null;
      const bindings = iamDb(db);
      const bound = await bindings
        .prepare(
          "INSERT INTO iam_oidc_subjects VALUES(?,?,?,?) ON CONFLICT(connection_id,issuer,subject) DO UPDATE SET external_id=iam_oidc_subjects.external_id WHERE iam_oidc_subjects.external_id=excluded.external_id RETURNING external_id",
        )
        .bind(id, issuer, claims.sub, externalId)
        .first();
      if (!bound) return null;
      const userId = await directoryUser(id, externalId),
        env = await runtimeEnv();
      const user = await env.CONTROL_DB.prepare(
        'SELECT name,email FROM "user" WHERE id=?',
      )
        .bind(userId)
        .first<{ name: string; email: string }>();
      if (!user) return null;
      return {
        ...claims,
        id: externalId,
        name: user.name,
        email: user.email,
        emailVerified: true,
      };
    },
  };
}
export async function localCredentialAllowed(userId: string) {
  if (userId.startsWith("dir_")) return false;
  // Company migration removes company authorization, rather than disabling an
  // unrelated company's local identity on the shared authentication account.
  return true;
}
