import { cookies } from "next/headers";
import { currentAccountProfile } from "@/server/modules/identity_access/public/account-profile";
import { appearancePreference } from "@/lib/shared/appearance-preference";
import { PageContent, Panel, Stack } from "@/components/ui/foundation";
import { AccountProfileForm } from "./account-profile-form";
import {AccountShell} from './account-shell';

export async function AccountHome() {
  const { accountKey, profile } = await currentAccountProfile(), cookieStore = await cookies();
  const preferences = profile.preferences.version ? profile.preferences : { ...profile.preferences,
    language: cookieStore.get("erpx_language")?.value === "th" ? "th" as const : "en" as const,
    appearance: appearancePreference(cookieStore.get("erpx_appearance")?.value) };
  const language = preferences.language, th = language === "th";
  return <AccountShell language={language} current="profile">
    <PageContent as="main"><Stack><h1>{th ? "บัญชีของคุณ" : "Your account"}</h1>
      <Panel appearance="standard"><AccountProfileForm key={accountKey} accountKey={accountKey} initial={{ ...profile, preferences }} language={language} /></Panel>
    </Stack></PageContent>
  </AccountShell>;
}
