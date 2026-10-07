"use client";
import { SectionHeader, Card } from "@/components/ui/foundation";

import { InlineNotice } from "@/components/ui/foundation";

import { Button, Field, Select, TextArea, TextInput } from "@/components/ui/foundation";

import { RecordLink as Link } from "@/components/records/record-link";
import { useCallback, useEffect, useRef, useState, type FormEvent } from "react";
import { usePathname, useRouter, useSearchParams } from "next/navigation";
import { ChevronDown, Sparkles } from "lucide-react";
import { workFetch } from "@/lib/work-fetch";
import {
  financeAiDate,
  financeAiKinds,
  financeAiLabels,
  type FinanceAiKind,
  type FinanceAiResult,
} from "@/lib/finance-ai";
import type { financeAiWorkspace } from "@/server/modules/finance/public/finance-ai";
import styles from "./finance-workspace.module.css";

type Workspace = Awaited<ReturnType<typeof financeAiWorkspace>>;
type Proposal = Workspace["proposals"][number];
type Preparation = {
  kind: Exclude<FinanceAiKind, "receipt_extraction">;
  sourceId: string;
  periodId?: string;
  chartId?: string;
  filingMonth?: string;
  startDate?: string;
  endDate?: string;
};
function currentPreparations(
  path: string,
  query: URLSearchParams,
  timezone: string,
  chartId?: string,
  active?: string,
  periodId?: string,
  filingMonth?: string,
): Preparation[] {
  const invoiceId = path.match(/\/finance\/invoices\/([^/]+)$/)?.[1];
  if (invoiceId)
    return [
      { kind: "invoice_review", sourceId: invoiceId },
      {
        kind:
          query.get("kind") === "payable" || active === "payables"
            ? "payment_preparation"
            : "collection_draft",
        sourceId: invoiceId,
      },
    ];
  if (path.endsWith("/finance/bank") && query.get("match"))
    return [{ kind: "bank_match", sourceId: query.get("match")! }];
  if (path.endsWith("/finance/close") && (query.get("periodId") || periodId))
    return [
      { kind: "close_review", sourceId: query.get("periodId") ?? periodId! },
      { kind: "tax_review", sourceId: query.get("periodId") ?? periodId! },
    ];
  if (path.endsWith("/finance/tax") && periodId && chartId)
    return [
      {
        kind: "tax_review",
        sourceId: periodId,
        chartId,
        filingMonth: query.get("filingMonth") ?? filingMonth,
      },
    ];
  if (path.endsWith("/finance/reports") && (query.get("chart") || chartId))
    return [
      {
        kind: "report_explanation",
        sourceId: query.get("chart") ?? chartId!,
        periodId: query.get("periodId") ?? undefined,
        startDate: query.get("startDate") ?? undefined,
        endDate: query.get("endDate") ?? undefined,
      },
    ];
  if (path.endsWith("/finance"))
    return [{ kind: "today_brief", sourceId: financeAiDate(timezone) }];
  return [];
}
async function readWorkspace(api: string): Promise<Workspace> {
  const response = await workFetch(api, { cache: "no-store" }),
    body = await response.json();
  if (!response.ok) throw new Error(body.error?.message ?? "Company Finance AI is unavailable.");
  return body.data;
}
async function command(url: string, input: unknown, key: string) {
  const response = await workFetch(url, {
      method: "POST",
      headers: { "Content-Type": "application/json", "Idempotency-Key": key },
      body: JSON.stringify(input),
    }),
    result = await response.json();
  if (!response.ok)
    throw new Error(
      result.error?.message ?? "The action could not be completed. Your records are preserved.",
    );
  return result.data;
}
function SourceEvidence({ proposal }: { proposal: Proposal }) {
  return (
    <details className={styles.disclosure}>
      <summary>Source evidence · version {proposal.sourceVersion}</summary>
      <p className="muted">
        Prepared from the retained facts below. Attachments are read only in the receipt extraction
        workflow.
      </p>
      {proposal.sources.map((source) => (
        <section key={source.id}>
          <h4>{source.label}</h4>
          <dl className={styles.aiFacts}>
            {Object.entries(source.data).map(([key, value]) => (
              <div key={key}>
                <dt>{key}</dt>
                <dd>
                  {typeof value === "object" && value !== null ? (
                    <pre>{JSON.stringify(value, null, 2)}</pre>
                  ) : (
                    String(value ?? "Not provided")
                  )}
                </dd>
              </div>
            ))}
          </dl>
        </section>
      ))}
      <p className="muted">
        Evidence hash: <code className={styles.aiHash}>{proposal.sourceHash}</code>
      </p>
    </details>
  );
}
function DraftReview({
  proposal,
  api,
  onSaved,
}: {
  proposal: Proposal;
  api: string;
  onSaved: () => void;
}) {
  const source = proposal.sources[0]?.data,
    suggested = proposal.result?.accounts;
  const [reason, setReason] = useState(""),
    [busy, setBusy] = useState(false),
    [error, setError] = useState(""),
    [dirty, setDirty] = useState(false);
  const requestKey = useRef<{ body: string; key: string } | null>(null);
  if (!suggested || source?.status !== "draft" || source.source_type !== "standalone") return null;
  const available = (source.availableAccounts ?? []) as {
    id: string;
    code: string;
    name: string;
  }[];
  const accountName = (id: unknown) =>
    available.find((row) => row.id === id)?.name ?? String(id ?? "None");
  async function save(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setBusy(true);
    setError("");
    const fields = new FormData(event.currentTarget),
      accounts = {
        debitAccountId: String(fields.get("debit")),
        creditAccountId: String(fields.get("credit")),
        taxAccountId: String(fields.get("tax") ?? "") || null,
      },
      body = { expectedVersion: proposal.version, reason, accounts },
      encoded = JSON.stringify(body);
    if (requestKey.current?.body !== encoded)
      requestKey.current = { body: encoded, key: crypto.randomUUID() };
    try {
      await command(`${api}/${proposal.id}/apply-draft`, body, requestKey.current!.key);
      onSaved();
    } catch (error) {
      setError(error instanceof Error ? error.message : "Draft could not be saved.");
    } finally {
      setBusy(false);
    }
  }
  return (
    <details className={styles.disclosure}>
      <summary>Review suggested accounts</summary>
      <form onSubmit={save} onChange={() => setDirty(true)} data-work-dirty={dirty}>
        <p className="muted">
          Review each change before saving. Amounts, dates and counterparties stay with the source
          document. Independent posting approval is still required.
        </p>
        <div className="form-grid">
          {(
            [
              ["debit", "Debit account", source.debit_account_id, suggested.debitAccountId],
              ["credit", "Credit account", source.credit_account_id, suggested.creditAccountId],
              ["tax", "Tax account", source.tax_account_id, suggested.taxAccountId],
            ] as const
          ).map(([key, label, before, after]) => (
            <Field appearance="standard" key={key}>
              {label}
              <small>Current: {accountName(before)}</small>
              <Select appearance="standard" name={key} defaultValue={after ?? ""} required={key !== "tax"}>
                {key === "tax" && <option value="">None</option>}
                {available.map((account) => (
                  <option key={account.id} value={account.id}>
                    {account.code} · {account.name}
                  </option>
                ))}
              </Select>
            </Field>
          ))}
          <Field appearance="standard">
            Review reason
            <TextArea appearance="standard"
              value={reason}
              onChange={(event) => setReason(event.target.value)}
              minLength={10}
              maxLength={2000}
              required
            />
          </Field>
        </div>
        {error && (
          <InlineNotice tone="danger" role="alert" className="error">
            {error}
          </InlineNotice>
        )}
        <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy || reason.trim().length < 10}>
          {busy ? "Saving draft…" : "Save reviewed draft"}
        </Button>
      </form>
    </details>
  );
}
function ProposalCard({
  proposal,
  api,
  base,
  canWrite,
  onSaved,
}: {
  proposal: Proposal;
  api: string;
  base: string;
  canWrite: boolean;
  onSaved: () => void;
}) {
  const [reason, setReason] = useState(""),
    [busy, setBusy] = useState(false),
    [error, setError] = useState(""),
    [copied, setCopied] = useState(false);
  const requestKey = useRef<{ body: string; key: string } | null>(null);
  async function act(action: "dismiss" | "retry") {
    setBusy(true);
    setError("");
    const body =
        action === "dismiss"
          ? { expectedVersion: proposal.version, reason }
          : { ...proposal.query, retry: true },
      encoded = JSON.stringify(body);
    if (requestKey.current?.body !== encoded)
      requestKey.current = { body: encoded, key: crypto.randomUUID() };
    try {
      await command(
        action === "dismiss" ? `${api}/${proposal.id}/dismiss` : `${api}/prepare`,
        body,
        requestKey.current!.key,
      );
      onSaved();
    } catch (error) {
      setError(error instanceof Error ? error.message : "The action failed.");
    } finally {
      setBusy(false);
    }
  }
  const result = proposal.result as FinanceAiResult | null;
  return (
    <article className={styles.aiProposal}>
      <SectionHeader className={styles.sectionHead}>
        <div>
          <p className="eyebrow">{financeAiLabels[proposal.kind]}</p>
          <h3>
            <Link href={`${base}/${proposal.href}`}>{proposal.sourceLabel}</Link>
          </h3>
        </div>
        <span className={styles.status}>{proposal.state}</span>
      </SectionHeader>
      {result ? (
        <>
          <p>{result.summary}</p>
          {result.checks.length > 0 && (
            <ul>
              {result.checks.map((check, index) => (
                <li key={index}>{check}</li>
              ))}
            </ul>
          )}
          {result.draftText && (
            <details className={styles.disclosure}>
              <summary>Review draft text</summary>
              <p className={styles.aiDraft}>{result.draftText}</p>
              <Button appearance="standard"
                variant="secondary"
                type="button"
                onClick={() =>
                  void navigator.clipboard
                    .writeText(result.draftText!)
                    .then(() => setCopied(true))
                    .catch(() => setError("Copy is unavailable. Select and copy the draft text."))
                }
              >
                {copied ? "Copied" : "Copy draft text"}
              </Button>
              <p className="muted">
                Review the recipient and facts before sending through your normal communication
                workflow.
              </p>
            </details>
          )}
          {result.bankPaymentId && (
            <p>
              Suggested compatible payment:{" "}
              <Link href={`${base}/finance/payments/${encodeURIComponent(result.bankPaymentId)}`}>
                {result.bankPaymentId}
              </Link>
              . Review the bank evidence before reconciliation.
            </p>
          )}
          <SourceEvidence proposal={proposal} />
          {canWrite && <DraftReview proposal={proposal} api={api} onSaved={onSaved} />}
        </>
      ) : (
        <p className="muted">
          {proposal.error ??
            (proposal.state === "stale"
              ? "The source, policy or provider changed. Prepare fresh evidence or continue manually."
              : proposal.state === "queued" || proposal.state === "running"
                ? "Preparing from the retained source facts…"
                : "This decision is retained in the audit history.")}
        </p>
      )}
      {!result && proposal.sources.length > 0 && <SourceEvidence proposal={proposal} />}
      <details className={styles.disclosure}>
        <summary>Preparation & decision history</summary>
        {proposal.retainedResult && (
          <div>
            <p className="muted">Retained AI response, for audit only:</p>
            <pre>{JSON.stringify(proposal.retainedResult, null, 2)}</pre>
          </div>
        )}
        {proposal.events.map((event, index) => (
          <div key={index}>
            <p>
              <b>{event.action}</b> · {new Date(event.createdAt).toISOString()}
            </p>
            {event.evidence.reason != null && <p>{String(event.evidence.reason)}</p>}
            {event.evidence.selectedAccounts != null && (
              <pre>{JSON.stringify(event.evidence.selectedAccounts, null, 2)}</pre>
            )}
          </div>
        ))}
      </details>
      {!["applied", "dismissed", "running", "queued"].includes(proposal.state) && (
        <details className={styles.disclosure}>
          <summary>Proposal actions</summary>
          <Field appearance="standard" data-work-dirty={reason.length > 0}>
            Decision reason
            <TextArea appearance="standard"
              value={reason}
              onChange={(event) => setReason(event.target.value)}
              minLength={10}
              maxLength={2000}
            />
          </Field>
          <div className="ui-form-actions">
            <Button appearance="standard"
              variant="secondary"
              type="button"
              disabled={busy || reason.trim().length < 10}
              onClick={() => void act("dismiss")}
            >
              Dismiss proposal
            </Button>
            {["stale", "failed"].includes(proposal.state) && (
              <Button appearance="standard"
                variant="secondary"
                type="button"
                disabled={busy}
                onClick={() => void act("retry")}
              >
                Prepare fresh proposal
              </Button>
            )}
          </div>
        </details>
      )}
      {error && (
        <InlineNotice tone="danger" role="alert" className="error">
          {error}
        </InlineNotice>
      )}
    </article>
  );
}
export function FinanceAiPanel({
  base,
  chartId,
  active,
  periodId,
  filingMonth,
  initialOpen = false,
}: {
  base: string;
  chartId?: string;
  active?: string;
  periodId?: string;
  filingMonth?: string;
  initialOpen?: boolean;
}) {
  const pathname = usePathname(),
    query = useSearchParams(),
    router = useRouter(),
    [open, setOpen] = useState(initialOpen),
    [data, setData] = useState<Workspace | null>(null),
    [error, setError] = useState(""),
    [busy, setBusy] = useState(false),
    [visibleLimit, setVisibleLimit] = useState(6);
  const api = `/api/v1${base}/finance/ai`;
  const refresh = useCallback(async () => {
    try {
      const response = await workFetch(api, { cache: "no-store" }),
        body = await response.json();
      if (!response.ok) throw new Error(body.error?.message ?? "Finance AI is unavailable.");
      setData(body.data);
      setError("");
    } catch (error) {
      setError(error instanceof Error ? error.message : "Finance AI is unavailable.");
    }
  }, [api]);
  useEffect(() => {
    let cancelled = false;
    void readWorkspace(api)
      .then((value) => {
        if (!cancelled) {
          setData(value);
          setError("");
        }
      })
      .catch((error) => {
        if (!cancelled)
          setError(error instanceof Error ? error.message : "Finance AI is unavailable.");
      });
    return () => {
      cancelled = true;
    };
  }, [api]);
  const working = data?.proposals.some((row) => ["queued", "running"].includes(row.state));
  useEffect(() => {
    if (!open || !working) return;
    const timer = setInterval(() => void refresh(), 5000);
    return () => clearInterval(timer);
  }, [open, working, refresh]);
  const prepared = () => {
    void refresh();
    router.refresh();
  };
  const choices = data
    ? currentPreparations(
        pathname,
        new URLSearchParams(query),
        data.policy.timezone,
        chartId,
        active,
        periodId,
        filingMonth,
      ).filter((item) => data.policy.kinds.includes(item.kind))
    : [];
  const matching =
    data?.proposals.filter(
      (row) => choices.length === 0 || choices.some((item) => item.sourceId === row.sourceId),
    ) ?? [];
  const visible = matching.slice(0, visibleLimit);
  async function prepare(choice: Preparation) {
    setBusy(true);
    setError("");
    try {
      await command(`${api}/prepare`, choice, crypto.randomUUID());
      await refresh();
    } catch (error) {
      setError(error instanceof Error ? error.message : "Preparation failed.");
    } finally {
      setBusy(false);
    }
  }
  return (
    <details
      className={`${styles.disclosure} ${styles.aiPanel}`}
      open={open}
      onToggle={(event) => setOpen(event.currentTarget.open)}
    >
      <summary>
        <Sparkles size={18} aria-hidden="true" /> AI assistance
        {data && (
          <span className="muted">
            {data.policy.enabled
              ? `${data.providerModel ?? "Provider unavailable"} · ${data.proposals.filter((row) => row.state === "ready").length} ready`
              : "Automatic preparation is off"}
          </span>
        )}
        <ChevronDown size={16} className={styles.aiChevron} aria-hidden="true" />
      </summary>
      <div>
        {error && (
          <InlineNotice tone="danger" role="alert" className="error">
            {error}{" "}
            <Button appearance="standard" variant="secondary" type="button" onClick={() => void refresh()}>
              Retry loading
            </Button>
          </InlineNotice>
        )}
        {!data && !error && <p className="muted">Loading Company AI policy…</p>}
        {data && (
          <>
            <p className="muted">
              AI prepares suggestions and draft text. Review evidence before changing a record. You
              control approvals, posting, payments, reconciliation and closing.
            </p>
            {!data.policy.enabled && data.canManage && (
              <p>
                Enable selected preparations in{" "}
                <Link href={`${base}/finance/setup?step=ai`}>Finance AI policy</Link>. All workflows
                remain available manually.
              </p>
            )}
            {data.unavailable && <InlineNotice tone="info" role="status">{data.unavailable}</InlineNotice>}
            {data.policy.lastError && (
              <InlineNotice tone="info" role="status">Automatic preparation: {data.policy.lastError}</InlineNotice>
            )}
            {data.policy.enabled && (
              <>
                <div className="ui-form-actions">
                  {choices.map((choice) => (
                    <Button appearance="standard"
                      variant="secondary"
                      key={choice.kind}
                      type="button"
                      disabled={busy || !!data.unavailable}
                      onClick={() => void prepare(choice)}
                    >
                      Prepare {financeAiLabels[choice.kind].toLowerCase()}
                    </Button>
                  ))}
                  {data.canManage && (
                    <Link href={`${base}/finance/setup?step=ai`}>Company AI policy</Link>
                  )}
                </div>
                {visible.length === 0 && (
                  <p className="muted">
                    No proposals for this view yet. Automatic preparation runs only for enabled
                    capabilities and records accessible to the policy owner. Select a document, bank
                    movement, period or report to prepare one directly.
                  </p>
                )}
                {visible.map((proposal) => (
                  <ProposalCard
                    key={`${proposal.id}:${proposal.version}`}
                    proposal={proposal}
                    api={api}
                    base={base}
                    canWrite={data.canWrite}
                    onSaved={prepared}
                  />
                ))}
                {matching.length > visible.length && (
                  <Button appearance="standard"
                    variant="secondary"
                    type="button"
                    onClick={() => setVisibleLimit((limit) => limit + 6)}
                  >
                    Show more proposals
                  </Button>
                )}
                {matching.length > 0 && (
                  <p className="muted">
                    Showing {visible.length} of {matching.length} recent proposals. Source evidence
                    and decisions are retained.
                  </p>
                )}
              </>
            )}
          </>
        )}
      </div>
    </details>
  );
}
export function FinanceAiPolicyControls({ base }: { base: string }) {
  const [data, setData] = useState<Workspace | null>(null),
    [error, setError] = useState("");
  const load = useCallback(async () => {
    const response = await workFetch(`/api/v1${base}/finance/ai`, { cache: "no-store" }),
      body = await response.json();
    if (response.ok) setData(body.data);
    else setError(body.error?.message ?? "Company AI policy is unavailable.");
  }, [base]);
  useEffect(() => {
    let cancelled = false;
    void readWorkspace(`/api/v1${base}/finance/ai`)
      .then((value) => {
        if (!cancelled) setData(value);
      })
      .catch((error) => {
        if (!cancelled)
          setError(error instanceof Error ? error.message : "Company AI policy is unavailable.");
      });
    return () => {
      cancelled = true;
    };
  }, [base]);
  return (
    <Card as="section" appearance="standard" className={styles.panel}>
      <h2>Company Finance AI policy</h2>
      <p className="muted">
        Choose which preparations may run automatically under your current assignment. Changing
        roles in the console keeps this explicit delegation active; revoking the assignment or
        changing its access stops preparation.
      </p>
      {error && (
        <InlineNotice tone="danger" role="alert" className="error">
          {error}
        </InlineNotice>
      )}
      {data ? (
        <PolicyForm key={data.policy.version} data={data} base={base} saved={() => void load()} />
      ) : (
        !error && <p>Loading policy…</p>
      )}
    </Card>
  );
}
function PolicyForm({ data, base, saved }: { data: Workspace; base: string; saved: () => void }) {
  const [busy, setBusy] = useState(false),
    [error, setError] = useState(""),
    [reason, setReason] = useState(""),
    [enabled, setEnabled] = useState(data.policy.enabled),
    [dirty, setDirty] = useState(false);
  const requestKey = useRef<{ body: string; key: string } | null>(null);
  async function save(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setBusy(true);
    setError("");
    const form = new FormData(event.currentTarget),
      body = {
        expectedVersion: data.policy.version,
        enabled,
        kinds: form.getAll("kind"),
        timezone: String(form.get("timezone")),
        dailyLimit: Number(form.get("daily")),
        monthlyLimit: Number(form.get("monthly")),
        reason,
      },
      encoded = JSON.stringify(body);
    if (requestKey.current?.body !== encoded)
      requestKey.current = { body: encoded, key: crypto.randomUUID() };
    try {
      await command(`/api/v1${base}/finance/ai/policy`, body, requestKey.current!.key);
      saved();
    } catch (error) {
      setError(error instanceof Error ? error.message : "Policy could not be saved.");
    } finally {
      setBusy(false);
    }
  }
  return (
    <>
      <p>
        Provider: {data.providerModel ?? "Not configured"}
        {data.policy.ownerName && <> · Preparation owner: {data.policy.ownerName}</>}
      </p>
      {data.unavailable && (
        <InlineNotice tone="info" role="status">
          {data.unavailable}{" "}
          {data.canManage && (
            <Link href={`${base}/settings?section=ai`}>Review AI connections</Link>
          )}
        </InlineNotice>
      )}
      {data.policy.lastError && <InlineNotice tone="info" role="status">{data.policy.lastError}</InlineNotice>}
      <form onSubmit={save} onChange={() => setDirty(true)} data-work-dirty={dirty}>
        <fieldset disabled={!data.canManage || busy}>
          <legend>Automatic preparation</legend>
          <Field appearance="standard" orientation="inline" className={styles.aiCheck}>
            <TextInput appearance="standard"
              type="checkbox"
              checked={enabled}
              onChange={(event) => setEnabled(event.target.checked)}
            />
            Enable this Company policy
          </Field>
          <div className={styles.aiCapabilities}>
            {financeAiKinds.map((kind) => (
              <Field appearance="standard" orientation="inline" key={kind} className={styles.aiCheck}>
                <TextInput appearance="standard"
                  name="kind"
                  type="checkbox"
                  value={kind}
                  defaultChecked={data.policy.kinds.includes(kind)}
                />
                {financeAiLabels[kind]}
              </Field>
            ))}
          </div>
          <p className="muted">
            Receipt extraction uses the existing private-file OCR workflow and its current
            responsible person. Receipt PDF pages must be prepared before extraction. AI never
            changes financial state automatically.
          </p>
          <div className="form-grid">
            <Field appearance="standard">
              Business timezone
              <TextInput appearance="standard" name="timezone" defaultValue={data.policy.timezone} required />
            </Field>
            <Field appearance="standard">
              Preparations per day
              <TextInput appearance="standard"
                name="daily"
                type="number"
                min={1}
                max={50}
                defaultValue={data.policy.dailyLimit}
                required
              />
            </Field>
            <Field appearance="standard">
              Preparations per month
              <TextInput appearance="standard"
                name="monthly"
                type="number"
                min={1}
                max={1000}
                defaultValue={data.policy.monthlyLimit}
                required
              />
            </Field>
            <Field appearance="standard">
              Policy change reason
              <TextArea appearance="standard"
                value={reason}
                onChange={(event) => setReason(event.target.value)}
                minLength={10}
                maxLength={2000}
                required
              />
            </Field>
          </div>
          <p className="muted">
            Limits share the Company AI allowance. Each preparation attempt reserves a request.
            Disabling the policy makes outstanding automatic proposals stale.
          </p>
          <Button appearance="standard"
            clientManaged type="submit"
            variant="primary"
            disabled={busy || reason.trim().length < 10 || (enabled && !!data.unavailable)}
          >
            {busy ? "Saving policy…" : "Save Company policy"}
          </Button>
        </fieldset>
      </form>
      {!data.canManage && (
        <p className="muted">An authorized AI manager must change this policy.</p>
      )}
      {error && (
        <InlineNotice tone="danger" role="alert" className="error">
          {error}
        </InlineNotice>
      )}
    </>
  );
}
