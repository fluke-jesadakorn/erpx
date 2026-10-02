import { TimeText } from "@/components/time/time-preference-context";
import { FeaturePage, FeatureTabs,FeatureWidget,WidgetGrid } from "@/components/features/feature-page";
import { permissionDefinition } from "@/lib/permissions";
import { currentSession } from "@/server/authorization";
import { ownIamAccess } from "@/server/iam-my-access";
import { workspaceChoices } from "@/server/workspace-choices";
import { accountQuickAccessItems } from "@/server/account-quick-access";
import { readQuickAccessPreferences } from "@/server/quick-access-preferences";
import { QuickAccessShelf } from "@/components/quick-access/shelf";
import rail from "@/components/quick-access/area-rail.module.css";
import Link from "next/link";
import { redirect } from "next/navigation";
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
      className="content-stack"
      style={{ maxWidth: 1360, margin: "40px auto", padding: 24 }}
    >
      <h1>My access</h1>
      <div className={rail.layout}>
      <aside className={rail.rail} aria-label="Quick access">
        <QuickAccessShelf items={items} initialPreferences={preferences} defaultIds={items.map(item => item.id)} endpoint="/api/v1/account/quick-access/my-access" requestMode="plain" embedded placement="rail" />
      </aside>
      <div id="my-access-content" tabIndex={-1} className={rail.content}>
      <FeatureTabs/><WidgetGrid pageKey="account-my-access">
      <p>
        Each role is evaluated separately. Select a work role in your workspace
        to use its permissions.
      </p>
      <div className="button-row">
        <Link className="button secondary" href="/">
          Open workspace
        </Link>
        <Link className="button secondary" href="/security">
          Authenticator & sessions
        </Link>
      </div>
      {!access.length && (
        <FeatureWidget id="account-my-access-no-assignments-yet" title="No assignments yet"><section className="card">
          
          <p>
            Your account is signed in. Ask your company administrator to arrange
            reviewed access.
          </p>
        </section></FeatureWidget>
      )}
      <FeatureWidget id="account-roles" title="Roles & permissions">{access.map((a) => (
        <section className="card" key={a.id}>
          <h2>{a.role}</h2>
          <p>
            {a.company} · {a.scope}
          </p>
          <p>
            {a.active ? "Active" : "Unavailable"} · {a.identity} · expires{" "}
            <TimeText value={a.expiresAt} />
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
        </section>
      ))}</FeatureWidget>
    </WidgetGrid>
      </div>
      </div></FeaturePage>
    </main>
    </>
  );
}
