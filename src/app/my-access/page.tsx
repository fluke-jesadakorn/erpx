import { FeaturePage, FeatureTabs,FeatureWidget,WidgetGrid } from "@/components/features/feature-page";
import { permissionDefinition } from "@/lib/permissions";
import { currentSession } from "@/server/authorization";
import { ownIamAccess } from "@/server/iam-my-access";
import Link from "next/link";
import { redirect } from "next/navigation";
export default async function MyAccessPage() {
  const session = await currentSession().catch(() =>
    redirect("/login?next=/my-access"),
  );
  const access = await ownIamAccess(session.session.user.id);
  return (
    <FeaturePage
      className="content-stack"
      style={{ maxWidth: 1000, margin: "40px auto", padding: 24 }}
    >
      <h1>My access</h1><FeatureTabs/><WidgetGrid pageKey="account-my-access">
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
            {new Date(a.expiresAt).toISOString().slice(0, 10)}
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
    </WidgetGrid></FeaturePage>
  );
}
