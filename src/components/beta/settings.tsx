"use client";
import betaStyles from "@/components/beta/beta.module.css";
import { useEffect, useState, useRef } from "react";
import { betaFetch } from "@/lib/beta-client";
import type { AdminSection } from "@/lib/administration";
type Settings = {
  policy: {
    member_limit: number;
    branch_limit: number;
    ai_monthly_limit: number;
    storage_bytes: number;
  } | null;
  usage: { requests: number };
  files: { bytes: number };
  invites: {
    id: string;
    email: string;
    accepted_at: number | null;
    revoked_at: number | null;
    invitation_state: string;
    expires_at: number;
  }[];
  feedback: { id: string; body: string }[];
  reports: { id: string; post_id: string; reason: string; state: string }[];
  billing: { status: string } | null;
};
export function BetaSettings({
  base,
  owner,
  section,
  beta,
}: {
  base: string;
  owner: boolean;
  section: AdminSection;
  beta: boolean;
}) {
  const pending = useRef(new Map<string, string>());
  const api = `/api/v1${base}/beta`,
    [message, setMessage] = useState(""),
    [busy, setBusy] = useState(false),
    [settings, setSettings] = useState<Settings | null>(null),
    [meta, setMeta] = useState<{
      branches: { id: string; display_name: string }[];
      people: { id: string; name: string }[];
    }>({ branches: [], people: [] });
  async function reload() {
    setMeta(await betaFetch(`${api}/meta`));
    if (owner) setSettings(await betaFetch(`${api}/settings`));
  }
  useEffect(() => {
    let current = true;
    Promise.all([
      betaFetch<typeof meta>(`${api}/meta`),
      owner ? betaFetch<Settings>(`${api}/settings`) : Promise.resolve(null),
    ])
      .then(([m, s]) => {
        if (current) {
          setMeta(m);
          setSettings(s);
        }
      })
      .catch((e) => {
        if (current) setMessage(e.message);
      });
    return () => {
      current = false;
    };
  }, [api, owner]);
  async function submit(path: string, input: unknown) {
    setBusy(true);
    try {
      const requestId = JSON.stringify({ path, input });
      const commandKey = pending.current.get(requestId) ?? crypto.randomUUID();
      pending.current.set(requestId, commandKey);
      await betaFetch(`${api}/${path}`, input, commandKey);
      pending.current.delete(requestId);
      setMessage("Saved. Thank you.");
      await reload();
    } catch (e) {
      setMessage(e instanceof Error ? e.message : "Please try again.");
    } finally {
      setBusy(false);
    }
  }
  return (
    <div className={`beta-settings ${betaStyles.surface}`}>
      {section === "usage" && (
        <section className="card">
          <h2>{beta ? "Free public beta" : "Workspace usage"}</h2>
          {beta && (
            <p>
              No card required. Beta access never converts into a paid
              subscription automatically.
            </p>
          )}
          {settings && (
            <dl>
              <dt>AI requests this month</dt>
              <dd>
                {settings.usage.requests}
                {settings.policy && ` / ${settings.policy.ai_monthly_limit}`}
              </dd>
              <dt>Attached files</dt>
              <dd>
                {(settings.files.bytes / 1024 ** 2).toFixed(1)} MB
                {settings.policy &&
                  ` / ${(settings.policy.storage_bytes / 1024 ** 2).toFixed(0)} MB allowance`}
              </dd>
              {settings.policy && (
                <>
                  <dt>Workspace limits</dt>
                  <dd>
                    {settings.policy.member_limit} members ·{" "}
                    {settings.policy.branch_limit} branches
                  </dd>
                </>
              )}
              <dt>Billing</dt>
              <dd>
                {settings.billing
                  ? `Test subscription · ${settings.billing.status}`
                  : "No subscription configured"}
              </dd>
            </dl>
          )}
        </section>
      )}
      {section === "feedback" && (
        <section className="card">
          <h2>Tell us what needs to improve</h2>
          <form
            className={`beta-form ${betaStyles.surface}`}
            onSubmit={(e) => {
              e.preventDefault();
              submit("feedback", {
                body: new FormData(e.currentTarget).get("body"),
              });
            }}
          >
            <label>
              Your feedback
              <textarea
                name="body"
                placeholder="What were you trying to do? What got in the way?"
                required
                maxLength={3000}
              />
            </label>
            <button disabled={busy}>Send feedback</button>
          </form>
        </section>
      )}
      <p role="status">{message}</p>
      {owner && (
        <>
          {section === "people" && beta && (
            <section className="card">
              <h2>Invite your team</h2>
              <p>
                They will receive an email and join using their own verified
                account.
              </p>
              <form
                className={`beta-form ${betaStyles.surface}`}
                onSubmit={(e) => {
                  e.preventDefault();
                  const f = new FormData(e.currentTarget);
                  submit("invitations", {
                    email: f.get("email"),
                    branchId: f.get("branchId"),
                    role: f.get("role"),
                  });
                }}
              >
                <label>
                  Email
                  <input name="email" type="email" required />
                </label>
                <label>
                  Branch
                  <select name="branchId" required>
                    {meta.branches.map((b) => (
                      <option key={b.id} value={b.id}>
                        {b.display_name}
                      </option>
                    ))}
                  </select>
                </label>
                <label>
                  Company role
                  <select name="role">
                    <option value="employee">Employee</option>
                    <option value="it_admin">IT administrator</option>
                  </select>
                </label>
                <button disabled={busy}>Send invitation</button>
              </form>
              {settings?.invites.map((i) => (
                <p key={i.id}>
                  {i.email} ·{" "}
                  {i.invitation_state === "Invited"
                    ? `Invited · expires ${new Date(i.expires_at).toLocaleDateString()}`
                    : i.invitation_state}
                </p>
              ))}
            </section>
          )}
          {section === "organization" && beta && (
            <section className="card">
              <h2>Branches</h2>
              {meta.branches.map((b) => (
                <p key={b.id}>{b.display_name}</p>
              ))}
              <form
                className={`beta-form ${betaStyles.surface}`}
                onSubmit={(e) => {
                  e.preventDefault();
                  submit("branches", {
                    name: new FormData(e.currentTarget).get("name"),
                  });
                }}
              >
                <label>
                  New branch name
                  <input name="name" required minLength={2} maxLength={100} />
                </label>
                <button disabled={busy}>Add branch</button>
              </form>
            </section>
          )}

          {section === "feedback" && (
            <section className="card">
              <h2>Reports and feedback</h2>
              {settings?.reports.map((r) => (
                <p key={r.id}>
                  <a href={`${base}/posts/${r.post_id}`}>
                    Reported conversation
                  </a>{" "}
                  · {r.reason} · {r.state}
                </p>
              ))}
              {settings?.feedback.map((f) => (
                <p key={f.id}>{f.body}</p>
              ))}
            </section>
          )}
          {section === "usage" && (
            <details className="card">
              <summary>Stripe sandbox · Test controls</summary>
              <p>
                These controls use test mode only. They do not charge real money
                or change beta access.
              </p>
              <p>
                Test subscription:{" "}
                {settings?.billing?.status ?? "Not configured"}
              </p>
              {(["checkout", "portal"] as const).map((action) => (
                <button
                  className="button secondary"
                  key={action}
                  disabled={busy}
                  onClick={async () => {
                    setBusy(true);
                    try {
                      const data = await betaFetch<{ url: string }>(
                        `${api}/billing`,
                        { action },
                      );
                      window.location.assign(data.url);
                    } catch (e) {
                      setMessage(
                        e instanceof Error ? e.message : "Please try again.",
                      );
                      setBusy(false);
                    }
                  }}
                >
                  Test {action}
                </button>
              ))}
            </details>
          )}
        </>
      )}
    </div>
  );
}
