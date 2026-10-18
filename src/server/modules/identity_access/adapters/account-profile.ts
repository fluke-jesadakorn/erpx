import "server-only";
import { cache } from "react";
import { runtimeEnv } from "../../runtime/public/runtime";
import { ApiError } from "../../runtime/public/http";
import { currentSession } from "./authorization";
import { readAccountProfile } from "./account-preferences";
import { accountPreferenceKey } from "./account-preference-key";

/** Request-private read. The page receives only its own display profile. */
export const currentAccountProfile = cache(async () => {
  const { session } = await currentSession();
  const profile = await readAccountProfile((await runtimeEnv()).CONTROL_DB, session.user.id);
  if (!profile) throw new ApiError(401, "SIGN_IN_REQUIRED", "Sign in again to open your account.");
  return { accountKey: accountPreferenceKey(session.user.id), profile };
});
