import { cookies } from "next/headers";
import {holdingRoutingEnabled,holdingOrigin} from '@erpx/contracts/holding-hosts';
import {workspaceChoices} from '@erpx/server-organization/public/workspace-choices';
import { currentAccountProfile } from "@erpx/server-identity_access/public/account-profile";
import { appearancePreference } from "@erpx/feature-shared/lib/appearance-preference";
import { PageContent, Panel, Stack } from "@erpx/ui/components/foundation";
import { AccountProfileForm } from "@erpx/feature-identity_access/components/account-profile-form";
import {AccountShell} from '@erpx/feature-identity_access/components/account-shell';

export async function AccountHome() {
  const { accountKey, profile } = await currentAccountProfile(), cookieStore = await cookies();
  const preferences = profile.preferences.version ? profile.preferences : { ...profile.preferences,
    language: cookieStore.get("erpx_language")?.value === "th" ? "th" as const : "en" as const,
    appearance: appearancePreference(cookieStore.get("erpx_appearance")?.value) };
  const language = preferences.language, th = language === "th";
  const organizations=holdingRoutingEnabled(process.env)?(await workspaceChoices()).filter(choice=>choice.kind==='holding'&&holdingOrigin(choice.holdingSlug)):[];
  return <AccountShell language={language} current="profile">
    <PageContent as="main"><Stack><h1>{th ? "บัญชีของคุณ" : "Your account"}</h1>
      {organizations.length>0&&<Panel appearance="standard"><h2>{th?'องค์กรของคุณ':'Your organizations'}</h2><nav aria-label={th?'เลือกองค์กร':'Choose an organization'}>{organizations.map(choice=>choice.kind==='holding'&&<p key={choice.holdingSlug}><a data-product-switch href={holdingOrigin(choice.holdingSlug)!}>{choice.label}</a></p>)}</nav></Panel>}
      <Panel appearance="standard"><AccountProfileForm key={accountKey} accountKey={accountKey} initial={{ ...profile, preferences }} language={language} /></Panel>
    </Stack></PageContent>
  </AccountShell>;
}
