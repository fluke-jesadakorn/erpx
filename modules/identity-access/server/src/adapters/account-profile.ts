import "server-only";
import { cache } from "react";
import { ApiError } from "@erpx/platform-runtime-server/public/http";
import { currentSession } from "@erpx/identity-access-server/adapters/authorization";
import { nativeAccountProfile } from "@erpx/identity-access-server/public/account-profile-native";
import { accountPreferenceKey } from "@erpx/identity-access-server/adapters/account-preference-key";

/** Request-private read. The page receives only its own display profile. */
export const currentAccountProfile = cache(async () => {
  const { session } = await currentSession();
  const profile = await nativeAccountProfile(session.user.id);
  return { accountKey: accountPreferenceKey(session.user.id), profile };
});
