import Link from "next/link";
import { redirect } from "next/navigation";
import { currentSession } from "@/server/authorization";
import { ownIamAccess } from "@/server/iam-my-access";
import { permissionDefinition } from "@/lib/permissions";
export default async function MyAccessPage() {
  const session = await currentSession().catch(() =>
    redirect("/login?next=/my-access"),
  );
  const access = await ownIamAccess(session.session.user.id);
  return (
    <main
      className="content-stack"
      style={{ maxWidth: 1000, margin: "40px auto", padding: 24 }}
    >
      <h1>My access</h1>
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
        <section className="card">
          <h2>No assignments yet</h2>
          <p>
            Your account is signed in. Ask your company administrator to arrange
            reviewed access.
          </p>
        </section>
      )}
      {access.map((a) => (
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
      ))}
    </main>
  );
}
