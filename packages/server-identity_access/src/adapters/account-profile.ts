import "server-only";
import { cache } from "react";
import { runtimeEnv } from "../../../../packages/server-runtime/src/public/runtime";
import { ApiError } from "@erpx/server-runtime/public/http";
import { currentSession } from "@erpx/server-identity_access/adapters/authorization";
import { readAccountProfile } from "@erpx/server-identity_access/adapters/account-preferences";
import { accountPreferenceKey } from "@erpx/server-identity_access/adapters/account-preference-key";

/** Request-private read. The page receives only its own display profile. */
export const currentAccountProfile = cache(async () => {
  const { session } = await currentSession();
  const profile = await readAccountProfile((await runtimeEnv()).CONTROL_DB, session.user.id);
  if (!profile) throw new ApiError(401, "SIGN_IN_REQUIRED", "Sign in again to open your account.");
  return { accountKey: accountPreferenceKey(session.user.id), profile };
});
