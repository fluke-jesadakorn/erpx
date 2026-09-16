import { directoryAuthPlugin, directoryOidcConfig, localCredentialAllowed } from "./iam-auth";
import { genericOAuth } from "better-auth/plugins/generic-oauth";
import { requestPublicOrigin } from "@/server/public-origin";
import { drizzle } from "drizzle-orm/node-postgres";
import { drizzleAdapter } from "better-auth/adapters/drizzle";
import * as authSchema from "./db/auth-schema";
import "server-only";
import { PostgresDatabase } from "./runtime/postgres";

import { betterAuth } from "better-auth";
import { multiSession } from "better-auth/plugins";
import { sendBetaEmail } from "./beta-email";
import { runtimeEnv } from "./runtime";

export async function createAuth(requestOrigin?: string, directoryId?: string) {
  const env = await runtimeEnv();
  const local = env.ERPX_ENVIRONMENT === "local";
  const baseURL = local ? requestOrigin ?? env.BETTER_AUTH_URL : env.BETTER_AUTH_URL;
  const beta = env.ERPX_PUBLIC_BETA === "true";
  return betterAuth({
    database: drizzleAdapter(drizzle((env.CONTROL_DB as PostgresDatabase).pool),{provider:"pg",schema:authSchema,transaction:true}),
    baseURL,
    secret: env.BETTER_AUTH_SECRET,
    advanced: { useSecureCookies: !local, ...(!local ? { ipAddress: { ipAddressHeaders: ["cf-connecting-ip"] } } : {}) },
    emailAndPassword: {
      enabled: true,
      disableSignUp: !beta,
      requireEmailVerification: !local || beta,
      sendResetPassword: async ({user,url}) => { await sendBetaEmail(env,user.email,"Reset your ERPX password",url); },
      minPasswordLength: 12,
    },
    emailVerification: {
      sendOnSignUp: beta, autoSignInAfterVerification: true,
      sendVerificationEmail: async ({user,url}) => { await sendBetaEmail(env,user.email,"Verify your ERPX email",url); },
    },
    session: {
      expiresIn: 60 * 60 * 12,
      updateAge: 60 * 30,
    },
    rateLimit: {
      enabled: !local,
      storage: "database",
    },
    account: { encryptOAuthTokens:true, accountLinking: { enabled: false } },
    databaseHooks: { account: { create: { before: async account => account.providerId === "credential" && !await localCredentialAllowed(account.userId) ? false : undefined } } },
    plugins: [multiSession({ maximumSessions: 5 }), directoryAuthPlugin(), ...(directoryId ? [genericOAuth({config:[await directoryOidcConfig(directoryId)]})] : [])],
    trustedOrigins: [
      baseURL,
      env.BETTER_AUTH_URL,
      ...(local ? ["http://localhost:3100",
      "http://127.0.0.1:3100",
      "http://localhost:8787",
      "http://127.0.0.1:8787"] : []),
    ],
  });
}

export async function sessionFromRequest(request: Request) {
  const auth = await createAuth(requestPublicOrigin(request));
  return auth.api.getSession({ headers: request.headers });
}
