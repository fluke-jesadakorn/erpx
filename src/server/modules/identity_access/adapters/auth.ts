import {createPasswordResetCompleted} from '../application/auth-hooks';
import { authPolicy, legalInvitationAccountAllowed } from '../domain/auth';
import { directoryAuthPlugin, directoryOidcConfig, localCredentialAllowed } from "@/server/modules/identity_access/public/iam-auth";
import { genericOAuth } from "better-auth/plugins/generic-oauth";
import { requestPublicOrigin } from "@/server/modules/manage_erpx/public/public-origin";
import { drizzle } from "drizzle-orm/node-postgres";
import { drizzleAdapter } from "better-auth/adapters/drizzle";
import * as authSchema from "@/server/modules/runtime/public/db/auth-schema";
import "server-only";
import { PostgresDatabase } from "@/server/modules/runtime/public/runtime/postgres";
import { betterAuth } from "better-auth";
import { multiSession } from "better-auth/plugins";
import { runtimeEnv } from "@/server/modules/runtime/public/runtime";
import { queueAccountRecovery, recordAccountRecoveryCompleted, } from "@/server/modules/identity_access/public/account-recovery";
import { createHash } from "node:crypto";
import { configuredProductOrigin, productHostForHost, trustedProductOrigin } from "@/lib/runtime/product-hosts";
import { productSsoStore } from "./product-sso-store";
import { productSessionCookie, ssoOrigins } from "./product-sso";
type AuthRuntime = Awaited<ReturnType<typeof runtimeEnv>>;
type AuthFactory = Awaited<ReturnType<typeof buildAuth>>;
const ordinaryAuthFactories = new WeakMap<AuthRuntime, {
    database: AuthRuntime['CONTROL_DB'];
    pool: PostgresDatabase['pool'];
    factories: Map<string, Promise<AuthFactory>>;
}>();
export async function createAuth(requestOrigin?: string, directoryId?: string, legalInvitationEmail?: string) {
    const env = await runtimeEnv();
    const local = env.ERPX_ENVIRONMENT === "local";
    const beta = env.ERPX_PUBLIC_BETA === "true";
    const policy = authPolicy({ local, defaultOrigin: env.BETTER_AUTH_URL, pmsOrigin: process.env.ERPX_FRONT_PMS_ORIGIN, posOrigin: process.env.ERPX_FRONT_POS_ORIGIN, beta, legalInvitationEmail }, requestOrigin);
    const productOrigin = trustedProductOrigin(requestOrigin ?? "");
    if (productOrigin) {
        const audience = productHostForHost(new URL(productOrigin).host)!;
        policy.baseURL = productOrigin;
        policy.cookiePrefix = `erpx-${audience}`;
        policy.trustedOrigins = [productOrigin];
        policy.disableSignUp = !(audience==='account'&&legalInvitationEmail);
    }
    // Provider configuration and invitation identity are evaluated on every call.
    if (directoryId || legalInvitationEmail) return buildAuth(env, local, policy, directoryId, legalInvitationEmail);
    const pool = (env.CONTROL_DB as PostgresDatabase).pool;
    let bucket = ordinaryAuthFactories.get(env);
    if (!bucket || bucket.database !== env.CONTROL_DB || bucket.pool !== pool) {
        bucket = { database: env.CONTROL_DB, pool, factories: new Map() };
        ordinaryAuthFactories.set(env, bucket);
    }
    const key = createHash('sha256').update(JSON.stringify({ local, policy, secret: env.BETTER_AUTH_SECRET })).digest('hex');
    const existing = bucket.factories.get(key);
    if (existing) {
        bucket.factories.delete(key);
        bucket.factories.set(key, existing);
        return existing;
    }
    // Local origins may vary: bound initialization resources to sixteen policies.
    if (bucket.factories.size >= 16) bucket.factories.delete(bucket.factories.keys().next().value!);
    const created = buildAuth(env, local, policy).then(async auth => {
        await auth.$context;
        return auth;
    });
    bucket.factories.set(key, created);
    const factories = bucket.factories;
    void created.catch(() => { if (factories.get(key) === created) factories.delete(key); });
    // Only factory initialization is shared. Better Auth creates per-request
    // context and reads the joined user/session row on each getSession call.
    return created;
}
async function buildAuth(env: AuthRuntime, local: boolean, policy: ReturnType<typeof authPolicy>, directoryId?: string, legalInvitationEmail?: string) {
    const { baseURL, cookiePrefix } = policy;
    const audience = productHostForHost(new URL(baseURL).host);
    const sso = audience ? productSsoStore(env.CONTROL_DB as PostgresDatabase, "development", ssoOrigins()) : null;
    const auth = betterAuth({
        database: drizzleAdapter(drizzle((env.CONTROL_DB as PostgresDatabase).pool, { schema: authSchema }), { provider: "pg", schema: authSchema, transaction: true }),
        baseURL,
        secret: env.BETTER_AUTH_SECRET,
        advanced: { database: { joins: true }, cookiePrefix, useSecureCookies: !local || !!audience, ...(!local ? { ipAddress: { ipAddressHeaders: ["cf-connecting-ip"] } } : {}) },
        emailAndPassword: {
            enabled: true,
            disableSignUp: policy.disableSignUp,
            requireEmailVerification: policy.requireEmailVerification,
            sendResetPassword: async ({ user, url, token }) => {
                await queueAccountRecovery(env.CONTROL_DB, env, user, url, token);
            },
            resetPasswordTokenExpiresIn: 3600,
            revokeSessionsOnPasswordReset: true,
            onPasswordReset:async({user})=>createPasswordResetCompleted({record:(userId)=>recordAccountRecoveryCompleted(env.CONTROL_DB,userId),revokeSessions:async(userId)=>{await env.CONTROL_DB.prepare('DELETE FROM session WHERE "userId"=?').bind(userId).run();},auditFailure:(userId)=>console.error(JSON.stringify({event:"account_recovery_audit_failed",userId}))})(user.id),
            minPasswordLength: 12,
        },
        emailVerification: {
            sendOnSignUp: policy.sendOnSignUp, autoSignInAfterVerification: true,
            sendVerificationEmail: async ({ user, url }) => {
                const { sendBetaEmail } = await import("@/server/modules/manage_erpx/public/beta-email");
                await sendBetaEmail(env, user.email, `Verify your ${policy.verificationProduct} email`, url);
            },
        },
        session: {
            expiresIn: 60 * 60 * 12,
            updateAge: 60 * 30,
        },
        rateLimit: {
            enabled: !local,
            storage: "database",
        },
        account: { encryptOAuthTokens: true, accountLinking: { enabled: false } },
        databaseHooks: { ...(audience === "account" && sso ? { session: { create: { after: session => sso.registerCentral(session) } } } : {}), ...(legalInvitationEmail ? { user: { create: { before: async (user: {
                            email: string;
                        }) => legalInvitationAccountAllowed(user.email, legalInvitationEmail) ? undefined : false } } } : {}), account: { create: { before: async (account) => account.providerId === "credential" && !await localCredentialAllowed(account.userId) ? false : undefined } } },
        plugins: [multiSession({ maximumSessions: 5 }), directoryAuthPlugin(), ...(directoryId ? [genericOAuth({ config: [await directoryOidcConfig(directoryId)] })] : [])],
        trustedOrigins: policy.trustedOrigins,
    });
    if (audience && sso) {
        const getSession = auth.api.getSession;
        Object.assign(auth.api, { getSession: async (input: { headers: Headers }) => {
            // The request must still belong to this exact factory's audience.
            if (configuredProductOrigin(audience) !== baseURL || productHostForHost(input.headers.get("host")) !== audience) return null;
            if (audience !== "account") return sso.session(productSessionCookie(input.headers), audience);
            const session = await getSession(input);
            return session && await sso.centralIsCurrent(session.session.id) ? session : null;
        } });
    }
    return auth;
}
export async function sessionFromRequest(request: Request) {
    const auth = await createAuth(requestPublicOrigin(request));
    return auth.api.getSession({ headers: request.headers });
}
