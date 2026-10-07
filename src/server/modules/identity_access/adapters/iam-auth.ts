import { createDirectoryUser, createDirectorySignIn, createDirectoryOidcUserInfo } from '../application/directory-auth';
import { directorySubject } from '../domain/auth';
import type { ConnectionRow } from '../domain/directories';
import { iamThrottle } from "@/server/iam-throttle";
import "server-only";
import { z } from "zod";
import { decodeJwt } from "jose";
import { createAuthEndpoint, APIError } from "better-auth/api";
import { setSessionCookie } from "better-auth/cookies";
import type { GenericOAuthConfig } from "better-auth/plugins/generic-oauth";
import { iamDb } from "@/server/modules/identity_access/public/iam-db";
import { runtimeEnv } from "@/server/modules/runtime/public/runtime";
async function directoryUser(connectionId: string, externalId: string) {
    const { routedConnection, synchronizeConnection } = await import('@/server/modules/identity_access/public/iam-directories');
    let routed: Awaited<ReturnType<typeof routedConnection>>;
    return createDirectoryUser({ connection: async (id) => (routed = await routedConnection(id)).connection, synchronize: async (connection) => synchronizeConnection(routed!.db, connection), identity: async (connectionId, externalId, cutoff) => {
            const db = routed!.db;
            return await db
                .prepare(`SELECT i.user_id FROM iam_identities i JOIN iam_members m ON m.identity_id=i.id AND m.company_id=i.company_id AND m.user_id=i.user_id WHERE i.connection_id=? AND i.external_id=? AND i.enabled=true AND i.observed_at>? AND m.status='active' AND m.login_policy='directory'`)
                .bind(connectionId, externalId, cutoff)
                .first<{
                user_id: string;
            }>();
        }, now: Date.now, error: (code, message) => new APIError(code, { message }) })(connectionId, externalId);
}
export function directoryAuthPlugin() {
    return {
        id: "erpx-directory",
        endpoints: {
            directorySignIn: createAuthEndpoint("/directory/sign-in", {
                method: "POST",
                body: z.object({
                    connectionId: z.string().uuid(),
                    username: z.string().min(1).max(200),
                    password: z.string().min(1).max(1024),
                }),
            }, async (ctx) => {
                let native: {
                    routedConnection: typeof import('@/server/modules/identity_access/public/iam-directories').routedConnection;
                    verifyLdapPassword: typeof import('@/server/modules/identity_access/public/iam-providers').verifyLdapPassword;
                };
                const result = await createDirectorySignIn<ConnectionRow, NonNullable<Awaited<ReturnType<typeof ctx.context.internalAdapter.findUserById>>>, NonNullable<Awaited<ReturnType<typeof ctx.context.internalAdapter.createSession>>>>({ throttle: (connectionId, username) => iamThrottle('directory', connectionId + ':' + username), connection: async (id) => { const { routedConnection } = await import('@/server/modules/identity_access/public/iam-directories'); const { verifyLdapPassword } = await import('@/server/modules/identity_access/public/iam-providers'); native = { routedConnection, verifyLdapPassword }; return (await routedConnection(id)).connection; }, verify: (...args) => native.verifyLdapPassword(...args), approved: directoryUser, user: (id) => ctx.context.internalAdapter.findUserById(id), session: (id) => ctx.context.internalAdapter.createSession(id), error: (code, message) => new APIError(code, { message }) })(ctx.body);
                await setSessionCookie(ctx, result);
                return ctx.json({ success: true });
            }),
        },
    };
}
/** Register only the explicitly selected provider. An unavailable corporate issuer
 * must never prevent local sign-in, emergency recovery, or ordinary session reads. */
export async function directoryOidcConfig(id: string): Promise<GenericOAuthConfig> {
    const { routedConnection } = await import("@/server/modules/identity_access/public/iam-directories");
    const { directorySecret } = await import("@/server/modules/identity_access/public/iam-providers");
    const { connection } = await routedConnection(id);
    if (connection.status !== "active" || connection.config.mode !== "oidc")
        throw new APIError("FORBIDDEN", {
            message: "This sign-in provider is unavailable.",
        });
    const secret = await directorySecret(connection.config, connection.id), issuer = connection.config.issuer!;
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
        accountSubject: ({ profile }) => directorySubject(connection.provider, profile),
        getUserInfo: async (tokens) => createDirectoryOidcUserInfo({ decode: decodeJwt, bind: async (subject, externalId) => { const { db } = await routedConnection(id), bindings = iamDb(db); return !!await bindings.prepare('INSERT INTO iam_oidc_subjects VALUES(?,?,?,?) ON CONFLICT(connection_id,issuer,subject) DO UPDATE SET external_id=iam_oidc_subjects.external_id WHERE iam_oidc_subjects.external_id=excluded.external_id RETURNING external_id').bind(id, issuer, subject, externalId).first(); }, approved: (externalId) => directoryUser(id, externalId), user: async (userId) => {
                const env = await runtimeEnv();
                return env.CONTROL_DB.prepare('SELECT name,email FROM "user" WHERE id=?').bind(userId).first<{
                    name: string;
                    email: string;
                }>();
            } }, connection.provider, issuer, connection.config.tenantId)(tokens.idToken),
    };
}
import { localCredentialAllowed as credentialPolicy } from '../domain/auth';
export async function localCredentialAllowed(userId: string) { return credentialPolicy(userId); }
