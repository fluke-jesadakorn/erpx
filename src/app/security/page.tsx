import { accountSelfLoginHref, type EntryPageSearchParams } from '@/server/modules/identity_access/public/rules/page-entry-return';
import { FeaturePage, FeatureTabs,FeatureWidget,WidgetGrid } from "@/components/shared/features/feature-page";
import { currentSession } from "@/server/modules/identity_access/public/authorization";
import {AccountShell} from '@/components/identity_access/account-shell';
import {workspaceChoices} from '@/server/modules/organization/public/workspace-choices';
import { accountQuickAccessItems } from "@/server/modules/identity_access/public/account-quick-access";
import { readQuickAccessPreferences } from "@/server/modules/work_projection/public/quick-access-preferences";
import { QuickAccessShelf } from "@/components/work_projection/quick-access/shelf";
import rail from "@/components/work_projection/quick-access/area-rail.module.css";
import Link from "@/components/workplace/workspace-link";
import { redirect } from "next/navigation";
import { AccountSecurity } from "./security-form";
import {cookies,headers} from 'next/headers';
import {productHostForHost} from '@/lib/runtime/product-hosts';
import {currentAccountSecurity} from '@/server/modules/identity_access/public/account-security';
import {AccountSessions} from '@/components/identity_access/account-sessions';
import {preferredAccountLanguage} from '@/server/modules/identity_access/public/account-preference-snapshot';
export const dynamic='force-dynamic';
export default async function SecurityPage({ searchParams }: { searchParams?: Promise<EntryPageSearchParams> }) {
  let userId: string;
  try {
    userId = (await currentSession()).session.user.id;
  } catch {
    redirect(accountSelfLoginHref("/security", await searchParams ?? {}));
  }
  const accountHost=productHostForHost((await headers()).get('host'))==='account';
  const [choices,preferences]=await Promise.all([accountHost?Promise.resolve([]):workspaceChoices(),readQuickAccessPreferences(userId, "account-security", "account")]);
  const items = accountQuickAccessItems("security", { hasWorkspace: choices.length>0 });
  const account=accountHost?await currentAccountSecurity():null;
  const language=await preferredAccountLanguage((await cookies()).get('erpx_language')?.value==='th'?'th':'en');
  const content=(
    <>
    <a href="#security-content" className="skip-link">Skip to account security</a>
    <main>
    <FeaturePage
      className="ui-page-content"
      style={{ maxWidth: 1360, margin: "40px auto", padding: 24 }}
    >
      <h1>Account security</h1>
      <div className={rail.layout}>
      <aside className={rail.rail} aria-label="Quick access">
        <QuickAccessShelf appearance="standard" items={items} initialPreferences={preferences} defaultIds={items.map(item => item.id)} endpoint="/api/v1/account/quick-access/security" requestMode="plain" embedded placement="rail" />
      </aside>
      <div id="security-content" tabIndex={-1} className={rail.content}>
      <FeatureTabs/><WidgetGrid pageKey="account-security">
      <p>Manage your authenticator and activate protected emergency access.</p>
      <Link href="/my-access">View all my access</Link><FeatureWidget appearance="standard" id="account-security-controls" title="Account security"><AccountSecurity initialEmergency={account?.emergency}/>{account&&<AccountSessions key={account.security.accountKey} initial={account.security}/>}</FeatureWidget>
      <Link href={accountHost?'/my-profile':'/'}>{accountHost?'Return to your account':'Return to your workspace'}</Link>
    </WidgetGrid>
      </div>
      </div></FeaturePage>
    </main>
    </>
  );
  return accountHost?<AccountShell current="security" language={language}>{content}</AccountShell>:content;
}
