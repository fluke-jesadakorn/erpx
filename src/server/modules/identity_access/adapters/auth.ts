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
import { queueAccountRecovery, recordAccountRecoveryCompleted, } from "@/server/account-recovery";
export async function createAuth(requestOrigin?: string, directoryId?: string, legalInvitationEmail?: string) {
    const env = await runtimeEnv();
    const local = env.ERPX_ENVIRONMENT === "local";
    const beta = env.ERPX_PUBLIC_BETA === "true";
    const policy = authPolicy({ local, defaultOrigin: env.BETTER_AUTH_URL, pmsOrigin: process.env.ERPX_FRONT_PMS_ORIGIN, posOrigin: process.env.ERPX_FRONT_POS_ORIGIN, beta, legalInvitationEmail }, requestOrigin), { baseURL, cookiePrefix } = policy;
    return betterAuth({
        database: drizzleAdapter(drizzle((env.CONTROL_DB as PostgresDatabase).pool), { provider: "pg", schema: authSchema, transaction: true }),
        baseURL,
        secret: env.BETTER_AUTH_SECRET,
        advanced: { cookiePrefix, useSecureCookies: !local, ...(!local ? { ipAddress: { ipAddressHeaders: ["cf-connecting-ip"] } } : {}) },
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
        databaseHooks: { ...(legalInvitationEmail ? { user: { create: { before: async (user: {
                            email: string;
                        }) => legalInvitationAccountAllowed(user.email, legalInvitationEmail) ? undefined : false } } } : {}), account: { create: { before: async (account) => account.providerId === "credential" && !await localCredentialAllowed(account.userId) ? false : undefined } } },
        plugins: [multiSession({ maximumSessions: 5 }), directoryAuthPlugin(), ...(directoryId ? [genericOAuth({ config: [await directoryOidcConfig(directoryId)] })] : [])],
        trustedOrigins: policy.trustedOrigins,
    });
}
export async function sessionFromRequest(request: Request) {
    const auth = await createAuth(requestPublicOrigin(request));
    return auth.api.getSession({ headers: request.headers });
}
