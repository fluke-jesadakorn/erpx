
import { ActionLink as UiActionLink } from "@/components/ui/foundation";
import { TimeText } from "@/components/people_payroll/time/time-preference-context";
import { FeaturePage, FeatureTabs,FeatureWidget,WidgetGrid } from "@/components/shared/features/feature-page";
import { permissionDefinition } from "@/lib/identity_access/permissions";
import { currentSession } from "@/server/modules/identity_access/public/authorization";
import { ownIamAccess } from "@/server/modules/identity_access/public/iam-my-access";
import { workspaceChoices } from "@/server/modules/organization/public/workspace-choices";
import { accountQuickAccessItems } from "@/server/modules/identity_access/public/account-quick-access";
import { readQuickAccessPreferences } from "@/server/modules/work_projection/public/quick-access-preferences";
import { QuickAccessShelf } from "@/components/work_projection/quick-access/shelf";
import rail from "@/components/work_projection/quick-access/area-rail.module.css";
import { redirect } from "next/navigation";
import { Card } from "@/components/ui/foundation";
export default async function MyAccessPage() {
  const session = await currentSession().catch(() =>
    redirect("/login?next=/my-access"),
  );
  const [access, choices, preferences] = await Promise.all([
    ownIamAccess(session.session.user.id),
    workspaceChoices(),
    readQuickAccessPreferences(session.session.user.id, "account-my-access", "account"),
  ]);
  const items = accountQuickAccessItems("my-access", { hasWorkspace: choices.length > 0 });
  return (
    <>
    <a href="#my-access-content" className="skip-link">Skip to account access</a>
    <main>
    <FeaturePage
      className="ui-page-content"
      style={{ maxWidth: 1360, margin: "40px auto", padding: 24 }}
    >
      <h1>My access</h1>
      <div className={rail.layout}>
      <aside className={rail.rail} aria-label="Quick access">
        <QuickAccessShelf appearance="standard" items={items} initialPreferences={preferences} defaultIds={items.map(item => item.id)} endpoint="/api/v1/account/quick-access/my-access" requestMode="plain" embedded placement="rail" />
      </aside>
      <div id="my-access-content" tabIndex={-1} className={rail.content}>
      <FeatureTabs/><WidgetGrid pageKey="account-my-access">
      <p>
        Each role is evaluated separately. Select a work role in your workspace
        to use its permissions.
      </p>
      <div className="button-row">
        <UiActionLink appearance="standard" variant="secondary" href="/">
          Open workspace
        </UiActionLink>
        <UiActionLink appearance="standard" variant="secondary" href="/security">
          Authenticator & sessions
        </UiActionLink>
      </div>
      {!access.length && (
        <FeatureWidget appearance="standard" id="account-my-access-no-assignments-yet" title="No assignments yet"><Card as="section" appearance="standard" className="ui-panel">
          
          <p>
            Your account is signed in. Ask your company administrator to arrange
            reviewed access.
          </p>
        </Card></FeatureWidget>
      )}
      <FeatureWidget appearance="standard" id="account-roles" title="Roles & permissions">{access.map((a) => (
        <Card as="section" appearance="standard" className="ui-panel" key={a.id}>
          <h2>{a.role}</h2>
          <p>
            {a.company} · {a.scope}
          </p>
          <p>
            {a.active ? "Active" : "Unavailable"} · {a.identity} · expires{" "}
            <TimeText appearance="standard" value={a.expiresAt} />
          </p>
          <details>
            <summary>Permissions and explanations</summary>
            <ul>
              {a.decisions
                .filter((d) => d.result !== "unavailable")
                .map((d) => (
                  <li key={d.permission}>
                    <strong>{permissionDefinition(d.permission)?.label}</strong>
                    : {d.result}. {d.reasons.join(" ")}{" "}
                    {d.conditions.join("; ")}
                  </li>
                ))}
            </ul>
          </details>
        </Card>
      ))}</FeatureWidget>
    </WidgetGrid>
      </div>
      </div></FeaturePage>
    </main>
    </>
  );
}
