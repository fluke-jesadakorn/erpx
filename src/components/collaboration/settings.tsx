"use client";
import WorkspaceLink from '@/components/workplace/workspace-link';
import { canonicalUiHref } from "@/lib/work_projection/destination-registry";
import { InlineNotice } from "@/components/ui/foundation";

import { Button, Field, Select, TextArea, TextInput } from "@/components/ui/foundation";

import { BillingConsole } from "@/components/billing/billing-console";
import betaStyles from "@/components/collaboration/beta.module.css";
import { useEffect, useState, useRef } from "react";
import { betaFetch } from "@/lib/shared/beta-client";
import { TimeText } from "@/components/people_payroll/time/time-preference-context";
import type { AdminSection } from "@/lib/shared/administration";
import { Card } from "@/components/ui/foundation";
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
    <div className={`beta-settings ${betaStyles.surface}`} data-section={section}>
      {section === "usage" && beta && (
        <Card as="section" appearance="standard" className="ui-panel">
          <h2>{beta ? "Free public beta" : "Workspace usage"}</h2>
          {beta && (
            <p>
              No card required. Beta access never converts into a paid subscription automatically.
            </p>
          )}
          {settings && (
            <dl className={betaStyles.usageMetrics}>
              <div>
                <dt>AI requests this month</dt>
                <dd>
                  {settings.usage.requests}
                  {settings.policy && ` / ${settings.policy.ai_monthly_limit}`}
                </dd>
              </div>
              <div>
                <dt>Attached files</dt>
                <dd>
                  {(settings.files.bytes / 1024 ** 2).toFixed(1)} MB
                  {settings.policy &&
                    ` / ${(settings.policy.storage_bytes / 1024 ** 2).toFixed(0)} MB allowance`}
                </dd>
              </div>
              {settings.policy && (
                <div>
                  <dt>Workspace limits</dt>
                  <dd>
                    {settings.policy.member_limit} members · {settings.policy.branch_limit} branches
                  </dd>
                </div>
              )}
              <div>
                <dt>Legacy Sandbox history</dt>
                <dd>
                  {settings.billing
                    ? `Test subscription · ${settings.billing.status}`
                    : "No subscription configured"}
                </dd>
              </div>
            </dl>
          )}
        </Card>
      )}
      {section === "feedback" && (
        <Card as="section" appearance="standard" className="ui-panel">
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
            <Field appearance="standard">
              Your feedback
              <TextArea appearance="standard"
                name="body"
                placeholder="What were you trying to do? What got in the way?"
                required
                maxLength={3000}
              />
            </Field>
            <Button appearance="standard" clientManaged type="submit" variant="ghost" disabled={busy}>
              Send feedback
            </Button>
          </form>
        </Card>
      )}
      {message && <InlineNotice tone="info" role="status">{message}</InlineNotice>}
      {owner && (
        <>
          {section === "people" && beta && (
            <Card as="section" appearance="standard" className="ui-panel">
              <h2>Invite your team</h2>
              <p>They will receive an email and join using their own verified account.</p>
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
                <Field appearance="standard">
                  Email
                  <TextInput appearance="standard" name="email" type="email" required />
                </Field>
                <Field appearance="standard">
                  Branch
                  <Select appearance="standard" name="branchId" required>
                    {meta.branches.map((b) => (
                      <option key={b.id} value={b.id}>
                        {b.display_name}
                      </option>
                    ))}
                  </Select>
                </Field>
                <Field appearance="standard">
                  Company role
                  <Select appearance="standard" name="role">
                    <option value="employee">Employee</option>
                    <option value="it_admin">IT administrator</option>
                  </Select>
                </Field>
                <Button appearance="standard" clientManaged type="submit" variant="ghost" disabled={busy}>
                  Send invitation
                </Button>
              </form>
              {settings?.invites.map((i) => (
                <p key={i.id}>
                  {i.email} ·{" "}
                  {i.invitation_state === "Invited" ? (
                    <>
                      Invited · expires <TimeText appearance="standard" value={i.expires_at} />
                    </>
                  ) : (
                    i.invitation_state
                  )}
                </p>
              ))}
            </Card>
          )}
          {section === "organization" && beta && (
            <Card as="section" appearance="standard" className="ui-panel">
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
                <Field appearance="standard">
                  New branch name
                  <TextInput appearance="standard" name="name" required minLength={2} maxLength={100} />
                </Field>
                <Button appearance="standard" clientManaged type="submit" variant="ghost" disabled={busy}>
                  Add branch
                </Button>
              </form>
            </Card>
          )}

          {section === "feedback" && (
            <Card as="section" appearance="standard" className="ui-panel">
              <h2>Reports and feedback</h2>
              {settings?.reports.map((r) => (
                <p key={r.id}>
                  {r.state === "published" ? (
                    <WorkspaceLink href={canonicalUiHref(`${base}/posts/${r.post_id}`)}>Reported conversation</WorkspaceLink>
                  ) : (
                    <span>
                      {r.state === "archived" ? "Deleted by author" : "Hidden by moderator"} · audit
                      retained
                    </span>
                  )}{" "}
                  · {r.reason} · {r.state}
                </p>
              ))}
              {settings?.feedback.map((f) => (
                <p key={f.id}>{f.body}</p>
              ))}
            </Card>
          )}
          {section === "usage" && <BillingConsole base={base} writable />}
        </>
      )}
    </div>
  );
}
