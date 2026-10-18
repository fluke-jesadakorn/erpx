import "server-only";
import { cache } from "react";
import { appearancePreference } from "@/lib/shared/appearance-preference";
import { currentSession } from "./authorization";
import { currentAccountProfile } from "./account-profile";
import { accountPreferenceKey } from "./account-preference-key";
import { defaultAccountPreferences, type AccountPreferenceSnapshot } from "../domain/account-preferences";

/** One request-private projection, shared by the document and owner shells. */
export const accountPreferenceSnapshot = cache(async (): Promise<AccountPreferenceSnapshot | null> => {
  let key: string;
  try {
    const { session } = await currentSession();
    key = accountPreferenceKey(session.user.id);
  } catch (error) {
    return error instanceof Error && error.message === "UNAUTHENTICATED"
      ? { key: "anonymous", preference: { ...defaultAccountPreferences } } : null;
  }
  try {
    const { profile } = await currentAccountProfile();
    return { key, preference: profile.preferences };
  } catch {
    // An optional preference outage must not discard a business draft or
    // manufacture a new account identity/default persisted preference.
    return { key, preference: undefined };
  }
});

export async function preferredAccountAppearance(fallback: string | undefined) {
  const snapshot = await accountPreferenceSnapshot();
  return snapshot?.preference?.version ? snapshot.preference.appearance : appearancePreference(fallback);
}

export async function preferredAccountLanguage(fallback: "en" | "th") {
  const snapshot = await accountPreferenceSnapshot();
  return snapshot?.preference?.version ? snapshot.preference.language : fallback;
}
