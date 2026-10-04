import { FeaturePage, FeatureTabs,FeatureWidget,WidgetGrid } from "@/components/features/feature-page";
import { currentSession } from "@/server/authorization";
import { workspaceChoices } from "@/server/workspace-choices";
import { accountQuickAccessItems } from "@/server/account-quick-access";
import { readQuickAccessPreferences } from "@/server/quick-access-preferences";
import { QuickAccessShelf } from "@/components/quick-access/shelf";
import rail from "@/components/quick-access/area-rail.module.css";
import Link from "next/link";
import { redirect } from "next/navigation";
import { AccountSecurity } from "./security-form";
export default async function SecurityPage() {
  let userId: string;
  try {
    userId = (await currentSession()).session.user.id;
  } catch {
    redirect("/login?next=/security");
  }
  const [choices, preferences] = await Promise.all([
    workspaceChoices(),
    readQuickAccessPreferences(userId, "account-security", "account"),
  ]);
  const items = accountQuickAccessItems("security", { hasWorkspace: choices.length > 0 });
  return (
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
      <Link href="/my-access">View all my access</Link><FeatureWidget appearance="standard" id="account-security-controls" title="Account security"><AccountSecurity /></FeatureWidget>
      <Link href="/">Return to your workspace</Link>
    </WidgetGrid>
      </div>
      </div></FeaturePage>
    </main>
    </>
  );
}
