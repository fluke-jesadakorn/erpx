"use client";
import { FeatureTabBar,FeatureWidget,WidgetGrid } from "@/components/features/feature-page";
import { TargetBillingPanel } from "./target-billing-panel";
import { NextMonthForecast } from "./next-month-forecast";

import { betaFetch } from "@/lib/beta-client";
import {
baht,
billingMonth,
type BillingCost,
type BillingView,
type InvoiceLine,
} from "@/lib/billing";
import { APP_LOCALE,APP_TIMEZONE } from "@/lib/localization";
import { workFetch } from "@/lib/work-fetch";
import { useRouter,useSearchParams } from "next/navigation";
import { useCallback,useEffect,useRef,useState } from "react";
import Link from "next/link";
import styles from "./billing-console.module.css";
const date = (n: number | null) =>
  n
    ? new Date(n).toLocaleDateString(APP_LOCALE, {
        timeZone: APP_TIMEZONE,
        day: "numeric",
        month: "short",
        year: "numeric",
      })
    : "—";
const time = (n: number) =>
  new Date(n).toLocaleString(APP_LOCALE, { timeZone: APP_TIMEZONE });
const dayInput = (n: number) =>
  new Date(n + 7 * 3600000).toISOString().slice(0, 10);
const dayValue = (s: FormDataEntryValue | null) =>
  new Date(`${s}T00:00:00+07:00`).getTime();
type AccountChoice = { holding_id: string; display_name: string; slug: string };
export function BillingConsole({
  base,
  platform = false,
  writable = false,
}: {
  base?: string;
  platform?: boolean;
  writable?: boolean;
}) {
  const searchParams = useSearchParams(), router = useRouter();
  const tab = searchParams.get("tab") ?? "target";
  const holdingTarget = platform ? searchParams.get("holdingId") : null;
  const invoiceTarget = platform ? null : searchParams.get("invoice"),
    subscriptionTarget = platform ? null : searchParams.get("subscription");
  function setTab(value: string) { const query = new URLSearchParams(searchParams); query.set("tab", value); router.push(`?${query}`, {scroll:false}); }
  const [accounts, setAccounts] = useState<AccountChoice[]>([]),
    [holding, setHolding] = useState(""),
    [data, setData] = useState<BillingView | null>(null),
    
    [message, setMessage] = useState(""),
    [busy, setBusy] = useState(false),
    [reason, setReason] = useState(""),
    [periodId, setPeriodId] = useState(""),
    [consent, setConsent] = useState(false),
    [evidence, setEvidence] = useState<string | null>(null),
    [editingCost, setEditingCost] = useState<BillingCost | null>(null);
  const pending = useRef(
    new Map<string, { key: string; payload: Record<string, unknown> }>(),
  );
  const deepLinkHandled = useRef("");
  const api = platform
    ? `/api/v1/platform/billing${holding ? `?holdingId=${encodeURIComponent(holding)}` : ""}`
    : `/api/v1${base}/billing`;
  const reload = useCallback(async () => {
    if (platform && !holding) return;
    const d = await betaFetch<BillingView>(api);
    setData(d);
    setPeriodId((p) =>
      d.periods.some((v) => v.id === p) ? p : (d.periods[0]?.id ?? ""),
    );
  }, [api, platform, holding]);
  useEffect(() => {
    if (platform)
      betaFetch<{ accounts: AccountChoice[] }>("/api/v1/platform/billing")
        .then((d) => {
          setAccounts(d.accounts);
          setHolding(d.accounts.find((account) => account.holding_id === holdingTarget)?.holding_id ?? d.accounts[0]?.holding_id ?? "");
        })
        .catch((e) => setMessage(e.message));
  }, [platform, holdingTarget]);
  useEffect(() => {
    let active = true;
    const run = async () => {
      try {
        if (platform && !holding) return;
        const d = await betaFetch<BillingView>(api);
        if (active) {
          setData(d);
          setPeriodId((p) =>
            d.periods.some((v) => v.id === p) ? p : (d.periods[0]?.id ?? ""),
          );
        }
      } catch (e) {
        if (active) setMessage(e instanceof Error ? e.message : "Load failed");
      }
    };
    void run();
    const timer = setInterval(run, 15000);
    return () => {
      active = false;
      clearInterval(timer);
    };
  }, [api, platform, holding]);
  useEffect(() => {
    if (platform || !data) return;
    let targetId = "";
    if (invoiceTarget) {
      const invoice = data.invoices.find((entry) => entry.id === invoiceTarget);
      if (!invoice) return;
      targetId = "billing-invoice-" + invoice.id;
    } else if (subscriptionTarget === "enrollment") {
      targetId = "billing-subscription";
    } else if (subscriptionTarget === "legacy") {
      targetId = "billing-legacy-subscription";
    }
    if (!targetId || deepLinkHandled.current === targetId) return;
    const frame = window.requestAnimationFrame(() => {
      const invoice = invoiceTarget
        ? data.invoices.find((entry) => entry.id === invoiceTarget)
        : null;
      if (invoice && periodId !== invoice.period_id) {
        setPeriodId(invoice.period_id);
        return;
      }
      const target = document.getElementById(targetId);
      if (!target) return;
      target.scrollIntoView({ behavior: "smooth", block: "center" });
      deepLinkHandled.current = targetId;
    });
    return () => window.cancelAnimationFrame(frame);
  }, [data, invoiceTarget, periodId, platform, subscriptionTarget]);
  async function command(action: string, fields: Record<string, unknown> = {}) {
    if (!data || !writable) return false;
    if (reason.trim().length < 3) {
      setMessage("Enter a reason with at least 3 characters before saving.");
      return false;
    }
    setBusy(true);
    setMessage("");
    const digest = JSON.stringify({
      api,
      action,
      reason: reason.trim(),
      fields,
    });
    // Keep the original version and key after a lost response, even when polling refreshes the account.
    const saved = pending.current.get(digest) ?? {
      key: crypto.randomUUID(),
      payload: {
        action,
        version: data.account?.version ?? 0,
        reason: reason.trim(),
        ...fields,
      },
    };
    pending.current.set(digest, saved);
    try {
      const response = await workFetch(api, {
        method: "POST",
        headers: {
          "Content-Type": "application/json",
          "Idempotency-Key": saved.key,
        },
        body: JSON.stringify(saved.payload),
      });
      const body = await response.json();
      if (!response.ok) {
        if (response.status < 500) pending.current.delete(digest);
        throw new Error(body.error?.message ?? "Please retry");
      }
      const r = body.data as { url?: string };
      pending.current.delete(digest);
      if (r.url) {
        window.location.assign(r.url);
        return true;
      }
      await reload();
      setMessage("Saved · Sandbox only");
      return true;
    } catch (e) {
      setMessage(e instanceof Error ? e.message : "Please retry");
      await reload().catch(() => {});
      return false;
    } finally {
      setBusy(false);
    }
  }
  const period = data?.periods.find((v) => v.id === periodId),
    e = data?.enrollment,
    active = !!e && !e.ended_at;
  const plans =
    data?.plans.filter(
      (p) =>
        p.effective_month <= billingMonth(data.now) &&
        !data.plans.some(
          (v) =>
            v.id === p.id &&
            v.effective_month <= billingMonth(data.now) &&
            v.revision > p.revision,
        ),
    ) ?? [];
  const current = data?.periods.find((p) => p.month === billingMonth(data.now));
  const currentCosts =
    data?.costs.filter((c) => c.period_id === current?.id) ?? [];
  const outstanding =
    data?.invoices
      .filter((i) => i.status === "open")
      .reduce((n, i) => n + i.total_satang, 0) ?? 0;
  const periodCosts = data?.costs.filter((c) => c.period_id === periodId) ?? [];
  return (
    <section
      className={styles.root}
      aria-label={platform ? "Customer billing" : "Holding billing"}
    >
      <header className={styles.header}>
        <div>
          <h2>{platform ? "Customer billing" : "Usage & billing"}</h2>
          <div className={styles.muted}>
            One account per Holding · All companies and branches · THB · Asia/Bangkok
          </div>
        </div>
        <span className={styles.tag}>SANDBOX</span>
      </header>
      <div className={styles.sandbox}>
        Sandbox only. Existing Support billing and the proposed seat-and-usage model are reviewed separately. No live collection or new storage entitlement. Customer-paid AI through BYOK is excluded.
      </div>
      <Link className={`button secondary ${styles.usageLink}`} href={platform ? "/holdings/erpx/manage-erpx/billing/usage" : `${base}/usage`}>
        View measured AI and file usage
      </Link>
      {platform && (
        <label>
          Customer account
          <select
            value={holding}
            onChange={(v) => {
              setHolding(v.target.value);
              setData(null);
              setEvidence(null);
            }}
          >
            {accounts.map((a) => (
              <option value={a.holding_id} key={a.holding_id}>
                {a.display_name} · {a.slug}
              </option>
            ))}
          </select>
        </label>
      )}
      {platform && <FeatureTabBar aria-label="Billing sections">{[["target","Measured usage & target"],["overview","Legacy Support"],["plans","Legacy plans"],["invoices","Costs & billing periods"],["payment","Payment"],["history","History & changes"]].map(([id,label])=><button key={id} aria-current={tab===id?"page":undefined} onClick={()=>setTab(id)}>{label}</button>)}</FeatureTabBar>}
      {!platform && data && <section id="billing-target" className={styles.targetPinned} aria-label="Measured usage and target billing">
        <NextMonthForecast base={base ?? "/holdings/erpx"} platform={false} holdingId={data.account?.holding_id ?? ""} />
        <TargetBillingPanel
          key={data.account?.holding_id ?? "holding"}
          base={base ?? "/holdings/erpx"}
          platform={false}
          holdingId={data.account?.holding_id ?? ""}
          now={data.now}
          writable={false}
          reason={reason}
          reasonValid={false}
          invoices={data.invoices}
        />
      </section>}
      <WidgetGrid pageKey={platform?"platform-billing":"administration-usage"}>
      {message && (
        <p role="status" className={styles.status}>
          {message}
        </p>
      )}
      {!data ? (
        <p>Loading billing account…</p>
      ) : (
        <>
          {writable && (
            <label>
              Action reason (saved in history)
              <input
                value={reason}
                onChange={(e) => setReason(e.target.value)}
                placeholder="e.g. Test the Standard plan in Sandbox"
                maxLength={1000}
              />
            </label>
          )}
          {platform && tab === "target" && <FeatureWidget id="billing-target" title="Measured usage & target billing"><div className={styles.forecastStack}>
            <NextMonthForecast key={`forecast:${holding}`} base={base ?? "/holdings/erpx"} platform holdingId={holding} />
            <TargetBillingPanel
            key={platform ? holding : data.account?.holding_id ?? "holding"}
            base={base ?? "/holdings/erpx"}
            platform={platform}
            holdingId={platform ? holding : data.account?.holding_id ?? ""}
            now={data.now}
            writable={platform && writable}
            reason={reason}
            reasonValid={reason.trim().length >= 3}
            invoices={data.invoices}
            onCommand={platform && writable ? command : undefined}
          /></div></FeatureWidget>}
          {(!platform || tab === "overview") && <FeatureWidget id="billing-overview" title="Legacy Support overview"><>
              <div className={styles.grid}>
                <article className={styles.card} id="billing-subscription">
                  <h3>Legacy Support plan</h3>
                  <div className={styles.metric}>
                    {active
                      ? plans.find((p) => p.id === e.plan_id)?.name
                      : "Not enrolled"}
                  </div>
                  <p className={styles.muted}>
                    {e?.ended_at
                      ? `Hosted ends (simulated) ${date(e.ended_at)} · Data and workspace remain available`
                      : active
                        ? "Legacy Support terms · No per-user charges"
                        : "Choose a plan to start the trial period"}
                  </p>
                </article>
                <article className={styles.card}>
                  <h3>Legacy Support fee</h3>
                  <div className={styles.metric}>
                    {baht(current?.support_satang ?? 0)}
                  </div>
                  <p className={styles.muted}>First month prorated from the start date</p>
                </article>
                <article className={styles.card}>
                  <h3>Recorded costs</h3>
                  <div className={styles.metric}>
                    {baht(
                      currentCosts.reduce((n, c) => n + c.amount_satang, 0),
                    )}
                  </div>
                  <p className={styles.muted}>
                    {current?.costs_complete
                      ? "Cost review complete"
                      : "Cost review pending · This is not the final total"}
                  </p>
                </article>
                <article className={styles.card}>
                  <h3>Outstanding invoices</h3>
                  <div className={styles.metric}>{baht(outstanding)}</div>
                  <p className={styles.muted}>Issued Stripe invoices only</p>
                </article>
              </div>
              <article className={styles.card}>
                <h3>Legacy estimate & pending items</h3>
                <p>
                  This period&apos;s estimate{" "}
                  {baht(
                    (current?.support_satang ?? 0) +
                      currentCosts.reduce((n, c) => n + c.amount_satang, 0),
                  )}{" "}
                  · Not an invoice
                </p>
                <p>
                  Costs awaiting review{" "}
                  {baht(
                    data.costs
                      .filter(
                        (c) =>
                          !c.invoice_id &&
                          !data.periods.find((p) => p.id === c.period_id)
                            ?.costs_complete,
                      )
                      .reduce((n, c) => n + c.amount_satang, 0),
                  )}
                </p>
                <p>
                  {e?.effective_at
                    ? `${e.pending_end ? "Hosted ends" : `Changes to ${plans.find((p) => p.id === e.pending_plan)?.name}`} effective ${date(e.effective_at)}`
                    : "No pending changes"}
                </p>
                {!platform && e?.effective_at && (
                  <button disabled={busy} onClick={() => command("withdraw")}>
                    Withdraw pending change
                  </button>
                )}
              </article>
            </></FeatureWidget>}
          {(!platform || tab === "plans") && <FeatureWidget id="billing-plans" title="Plans"><>
              <div className={styles.grid}>
                {plans.map((p) => (
                  <article key={p.id} className={styles.card}>
                    <h3>{p.name}</h3>
                    <div className={styles.metric}>
                      {baht(p.support_satang)}
                      <small className={styles.muted}> / month</small>
                    </div>
                    <p>+ Actual costs with no markup</p>
                    <p className={styles.muted}>
                      {p.id === "infrastructure"
                        ? "Hosted without Support"
                        : p.id === "standard"
                          ? "Standard Support"
                          : "Priority Support"}{" "}
                      · No SLA or ticketing in this period
                    </p>
                    <p className={styles.muted}>revision {p.revision}</p>
                    {!platform && (
                      <button
                        disabled={
                          busy || !writable || (active && e.plan_id === p.id)
                        }
                        onClick={() =>
                          command(active ? "change-plan" : "enroll", {
                            planId: p.id,
                          })
                        }
                      >
                        {active
                          ? e.plan_id === p.id
                            ? "Current plan"
                            : "Change next month"
                          : "Enroll in Sandbox"}
                      </button>
                    )}
                  </article>
                ))}
              </div>
              {data.plans
                .filter(
                  (p) =>
                    p.effective_month > billingMonth(data.now) &&
                    !data.plans.some(
                      (v) =>
                        v.id === p.id &&
                        v.effective_month === p.effective_month &&
                        v.revision > p.revision,
                    ),
                )
                .map((p) => (
                  <p key={`${p.id}-${p.revision}`} className={styles.status}>
                    {p.name} revision {p.revision} · Support{" "}
                    {baht(p.support_satang)} effective in period {p.effective_month}
                  </p>
                ))}
              {!platform && active && (
                <article className={styles.card}>
                  <h3>Manage services</h3>
                  <p>
                    Cancelling Support keeps Hosted active while actual costs
                    continue. Cancelling Hosted takes effect at month end.
                    Existing invoices and costs incurred before the end remain due.
                  </p>
                  <div className={styles.actions}>
                    <button
                      disabled={busy}
                      onClick={() =>
                        command("change-plan", { planId: "infrastructure" })
                      }
                    >
                      Cancel Support at month end
                    </button>
                    <button
                      className={styles.danger}
                      disabled={busy}
                      onClick={() => command("cancel-hosted")}
                    >
                      Request Hosted cancellation at month end (simulated)
                    </button>
                    {e.effective_at && (
                      <button
                        disabled={busy}
                        onClick={() => command("withdraw")}
                      >
                        Withdraw request
                      </button>
                    )}
                  </div>
                  <p className={styles.muted}>
                    Sandbox shows the impact only; it does not delete data or close the workspace.
                  </p>
                </article>
              )}
              {platform && writable && (
                <form
                  className={`${styles.card} ${styles.form}`}
                  onSubmit={(ev) => {
                    ev.preventDefault();
                    const f = new FormData(ev.currentTarget);
                    void command("revise-plan", {
                      planId: f.get("planId"),
                      planRevision: Math.max(
                        ...data.plans
                          .filter((p) => p.id === f.get("planId"))
                          .map((p) => p.revision),
                      ),
                      supportSatang: Math.round(Number(f.get("amount")) * 100),
                    });
                  }}
                >
                  <h3>New Support plan revision</h3>
                  <label>
                    Plan
                    <select name="planId">
                      {plans.map((p) => (
                        <option key={p.id} value={p.id}>
                          {p.name}
                        </option>
                      ))}
                    </select>
                  </label>
                  <label>
                    Support (THB / month)
                    <input
                      name="amount"
                      type="number"
                      min="0"
                      step="0.01"
                      required
                    />
                  </label>
                  <p className={styles.muted}>
                    Effective next month; prior periods and invoices are unchanged.
                  </p>
                  <button disabled={busy}>Save new revision</button>
                </form>
              )}
            </></FeatureWidget>}
          {(!platform || tab === "invoices") && <FeatureWidget id="billing-invoices" title="Costs & invoices"><>
              <label>
                Service period
                <select
                  value={periodId}
                  onChange={(ev) => {
                    setPeriodId(ev.target.value);
                    setEvidence(null);
                    setEditingCost(null);
                  }}
                >
                  {data.periods.map((p) => (
                    <option value={p.id} key={p.id}>
                      {p.month} ·{" "}
                      {p.closed_at ? "Month closed" : "Current month"} ·{" "}
                      {p.costs_complete ? "Costs complete" : "Cost review pending"}
                    </option>
                  ))}
                </select>
              </label>
              {period ? (
                <article className={styles.card}>
                  <h3>
                    {period.month} · {date(period.start_at)} –{" "}
                    {date(period.end_at - 1)}
                  </h3>
                  <p>
                    Support {baht(period.support_satang)} · plan revision{" "}
                    {period.plan_revision} ·{" "}
                    {period.costs_complete ? "Costs complete" : "Cost review pending"}
                  </p>
                  <p>
                    <strong>
                      Draft amount for approval:{" "}
                      {baht(
                        (data.invoices.some(
                          (i) =>
                            i.period_id === period.id &&
                            i.kind === "main" &&
                            i.status !== "void",
                        )
                          ? 0
                          : period.support_satang) +
                          periodCosts
                            .filter((c) => !c.invoice_id)
                            .reduce((sum, c) => sum + c.amount_satang, 0),
                      )}
                    </strong>{" "}
                    · Uninvoiced costs + unbilled Support
                  </p>
                  <div className={styles.table}>
                    <table>
                      <thead>
                        <tr>
                          <th>Cost</th>
                          <th>Service period</th>
                          <th>Amount</th>
                          <th>Status</th>
                        </tr>
                      </thead>
                      <tbody>
                        {periodCosts.map((c) => (
                          <tr key={c.id}>
                            <td>
                              {c.category}{" "}
                              {c.simulated && (
                                <span className={styles.tag}>Simulated</span>
                              )}
                              <p>{c.note}</p>
                              {platform && c.evidence_id && (
                                <a
                                  href={`/api/v1/platform/billing/evidence/${c.evidence_id}?holdingId=${holding}`}
                                >
                                  Private evidence
                                </a>
                              )}
                            </td>
                            <td>
                              {date(c.service_start)} –{" "}
                              {date(c.service_end - 1)}
                            </td>
                            <td>{baht(c.amount_satang)}</td>
                            <td>
                              {c.invoice_id
                                ? "Attached to invoice"
                                : "Draft / pending review"}
                              {platform && writable && !c.invoice_id && (
                                <>
                                  <button
                                    disabled={busy}
                                    onClick={() => {
                                      setEditingCost(c);
                                      setEvidence(c.evidence_id);
                                    }}
                                  >
                                    Edit draft
                                  </button>
                                  <button
                                    disabled={busy}
                                    onClick={() =>
                                      command("delete-cost", { costId: c.id })
                                    }
                                  >
                                    Delete draft
                                  </button>
                                </>
                              )}
                            </td>
                          </tr>
                        ))}
                      </tbody>
                    </table>
                  </div>
                  {!periodCosts.length && (
                    <p className={styles.muted}>
                      No costs have been recorded. This does not mean the month has zero cost.
                    </p>
                  )}
                  {platform && writable && (
                    <>
                      <details
                        className={styles.card}
                        open={editingCost ? true : undefined}
                      >
                        <summary>
                          {editingCost
                            ? "Edit cost draft"
                            : "Add cost / adjustment"}
                        </summary>
                        <form
                          className={styles.form}
                          key={`${period.id}-${editingCost?.id ?? "new"}`}
                          onSubmit={(ev) => {
                            ev.preventDefault();
                            const f = new FormData(ev.currentTarget);
                            void command("cost", {
                              periodId,
                              ...(editingCost
                                ? { costId: editingCost.id }
                                : {}),
                              category: f.get("category"),
                              amountSatang: Math.round(
                                Number(f.get("amount")) * 100,
                              ),
                              note: f.get("note"),
                              serviceStart: Math.max(
                                period.start_at,
                                dayValue(f.get("start")),
                              ),
                              serviceEnd: dayValue(f.get("end")) + 86400000,
                              simulated: f.get("simulated") === "on",
                              evidenceId: evidence,
                              ...(f.get("original")
                                ? { originalInvoiceId: f.get("original") }
                                : {}),
                            });
                          }}
                        >
                          <div className={styles.grid}>
                            <label>
                              Category
                              <select
                                name="category"
                                defaultValue={
                                  editingCost?.category ?? "hosting"
                                }
                              >
                                <option value="hosting">Hosting</option>
                                <option value="storage">Storage</option>
                                <option value="provider_ai">
                                  ERPX-paid AI (not BYOK)
                                </option>
                                <option value="adjustment">
                                  Invoice adjustment
                                </option>
                              </select>
                            </label>
                            <label>
                              Amount (THB)
                              <input
                                name="amount"
                                required
                                type="number"
                                step="0.01"
                                defaultValue={
                                  editingCost
                                    ? editingCost.amount_satang / 100
                                    : undefined
                                }
                              />
                            </label>
                            <label>
                              Start date
                              <input
                                name="start"
                                type="date"
                                defaultValue={dayInput(
                                  editingCost?.service_start ?? period.start_at,
                                )}
                                required
                              />
                            </label>
                            <label>
                              End date (inclusive)
                              <input
                                name="end"
                                type="date"
                                defaultValue={dayInput(
                                  (editingCost?.service_end ?? period.end_at) -
                                    1,
                                )}
                                required
                              />
                            </label>
                          </div>
                          <label>
                            Details / reason for late cost
                            <textarea
                              name="note"
                              required
                              maxLength={2000}
                              defaultValue={editingCost?.note ?? ""}
                            />
                          </label>
                          <label>
                            Reference original invoice
                            <select
                              name="original"
                              defaultValue={
                                editingCost?.original_invoice_id ?? ""
                              }
                            >
                              <option value="">None</option>
                              {data.invoices
                                .filter((i) => i.period_id === periodId)
                                .map((i) => (
                                  <option value={i.id} key={i.id}>
                                    {i.stripe_id ?? i.id} · {i.status}
                                  </option>
                                ))}
                            </select>
                          </label>
                          <label className={styles.check}>
                            <input
                              name="simulated"
                              type="checkbox"
                              defaultChecked={editingCost?.simulated ?? true}
                            />
                            Simulated cost data
                          </label>
                          <label>
                            Private evidence (PDF / PNG / JPEG, max 5 MB)
                            <input
                              type="file"
                              accept="application/pdf,image/png,image/jpeg"
                              onChange={async (ev) => {
                                const f = ev.target.files?.[0];
                                if (!f) return;
                                setBusy(true);
                                try {
                                  const r = await workFetch(
                                    `/api/v1/platform/billing/evidence?holdingId=${holding}`,
                                    {
                                      method: "POST",
                                      headers: {
                                        "Content-Type": f.type,
                                        "X-Filename": encodeURIComponent(
                                          f.name,
                                        ),
                                        "Idempotency-Key": crypto.randomUUID(),
                                      },
                                      body: f,
                                    },
                                  );
                                  const body = await r.json();
                                  if (!r.ok)
                                    throw new Error(body.error?.message);
                                  setEvidence(body.data.id);
                                  setMessage("Evidence attached");
                                } catch (e) {
                                  setMessage(
                                    e instanceof Error
                                      ? e.message
                                      : "Upload failed",
                                  );
                                } finally {
                                  setBusy(false);
                                }
                              }}
                            />
                          </label>
                          <div className={styles.actions}>
                            <button disabled={busy}>Save cost draft</button>
                            {editingCost && (
                              <button
                                type="button"
                                onClick={() => {
                                  setEditingCost(null);
                                  setEvidence(null);
                                }}
                              >
                                Add new item
                              </button>
                            )}
                          </div>
                        </form>
                      </details>
                      <div className={styles.actions}>
                        <button
                          disabled={busy}
                          onClick={() =>
                            command("complete-costs", {
                              periodId,
                              complete: !period.costs_complete,
                            })
                          }
                        >
                          {period.costs_complete
                            ? "Reopen cost review"
                            : "Confirm cost review complete"}
                        </button>
                        <button
                          className={styles.primary}
                          disabled={
                            busy || !period.closed_at || !period.costs_complete
                          }
                          onClick={() => command("approve", { periodId })}
                        >
                          Approve and issue invoice
                        </button>
                      </div>
                    </>
                  )}
                </article>
              ) : (
                <p>No service periods yet. Enroll in a plan from the Holding account.</p>
              )}
              <h3>Stripe invoices · Issued after month-end and cost approval</h3>
              {data.invoices
                .filter((i) => !periodId || i.period_id === periodId)
                .map((i) => (
                  <article className={styles.card} id={"billing-invoice-" + i.id} key={i.id}>
                    <div className={styles.header}>
                      <div>
                        <h3>{i.stripe_id ?? "Preparing Sandbox invoice"}</h3>
                        <span className={styles.tag}>{i.status}</span>{" "}
                        {i.payment_state && (
                          <span className={styles.tag}>{i.payment_state}</span>
                        )}
                        {i.status === "open" &&
                          i.due_at &&
                          i.due_at < data.now && (
                            <span className={styles.tag}>Past due</span>
                          )}
                      </div>
                      <strong>{baht(i.total_satang)}</strong>
                    </div>
                    <p className={styles.muted}>
                      Approved {date(i.created_at)} · Due {date(i.due_at)}
                      {i.original_invoice_id &&
                        ` · Adjustment of ${i.original_invoice_id}`}
                    </p>
                    {(JSON.parse(i.lines_json) as InvoiceLine[]).map((l, n) => (
                      <div className={styles.row} key={n}>
                        <div>
                          {l.description}
                          <p className={styles.muted}>
                            {date(l.start)} – {date(l.end - 1)}
                          </p>
                        </div>
                        <strong>{baht(l.amount)}</strong>
                      </div>
                    ))}
                    <div className={styles.actions}>
                      {!platform && i.hosted_url && (
                        <a href={i.hosted_url} target="_blank" rel="noreferrer">
                          {i.status === "open"
                            ? "Pay / confirm card at Stripe"
                            : "View invoice at Stripe"}
                        </a>
                      )}
                      {!platform && i.pdf_url && (
                        <a href={i.pdf_url} target="_blank" rel="noreferrer">
                          Download document
                        </a>
                      )}
                      {platform &&
                        writable &&
                        ["open", "uncollectible"].includes(i.status) && (
                          <button
                            disabled={busy}
                            onClick={() => command("void", { invoiceId: i.id })}
                          >
                            Void unpaid invoice
                          </button>
                        )}
                    </div>
                  </article>
                ))}
            </></FeatureWidget>}
          {(!platform || tab === "payment") && <FeatureWidget id="billing-payment" title="Payment"><article className={styles.card}>
              {data.account?.setup_state && (
                <p className={styles.status}>
                  Card setup: {data.account.setup_state}
                </p>
              )}
              <h3>
                {data.account?.payment_mode === "auto"
                  ? "Automatic card payment"
                  : "Manual invoice payment"}
              </h3>
              <p>
                Manual payment: due 7 days after issue · Auto-pay: charged
                after administrator approval
              </p>
              <p>
                Card:{" "}
                {data.account?.card_json
                  ? (() => {
                      const c = JSON.parse(data.account.card_json);
                      return `${c.brand} •••• ${c.last4} · ${c.exp_month}/${c.exp_year}`;
                    })()
                  : "No card saved"}
              </p>
              <p className={styles.muted}>
                Card details are entered and managed only at Stripe. ERPX
                stores only reference IDs and masked numbers. Payment status is
                updated from Stripe, not from the post-payment return page.
              </p>
              {!platform && (
                <>
                  <div className={styles.actions}>
                    <button disabled={busy} onClick={() => command("setup")}>
                      Set up test card
                    </button>
                    <button disabled={busy} onClick={() => command("portal")}>
                      Manage card and history at Stripe
                    </button>
                  </div>
                  <label className={styles.check}>
                    <input
                      type="checkbox"
                      checked={consent}
                      onChange={(ev) => setConsent(ev.target.checked)}
                    />
                    I agree to automatic charges after invoice review and approval (Sandbox)
                  </label>
                  <div className={styles.actions}>
                    <button
                      disabled={busy || !consent}
                      onClick={() =>
                        command("payment-mode", {
                          paymentMode: "auto",
                          consent: true,
                        })
                      }
                    >
                      Enable Auto-pay
                    </button>
                    <button
                      disabled={busy}
                      onClick={() =>
                        command("payment-mode", { paymentMode: "manual" })
                      }
                    >
                      Use manual payment / withdraw Auto-pay
                    </button>
                  </div>
                  <p className={styles.muted}>
                    Withdrawing Auto-pay stops the next charge, but does not
                    reverse a payment already initiated at Stripe.
                  </p>
                </>
              )}
              {data.legacy?.subscription_id && (
                <p id="billing-legacy-subscription">
                  Legacy test subscription: {data.legacy.subscription_id} ·{" "}
                  {data.legacy.status}
                </p>
              )}
            </article></FeatureWidget>}
          {(!platform || tab === "history") && <FeatureWidget id="billing-history" title="History & changes"><>
              <article className={styles.card}>
                <h3>Pending actions</h3>
                {!data.jobs.length && <p>No pending jobs</p>}
                {data.jobs.map((j) => (
                  <div key={j.id} className={styles.row}>
                    <span>
                      {j.state} · {j.last_error ?? "Waiting for worker"}
                    </span>
                    {platform && writable && j.state === "attention" && (
                      <button
                        disabled={busy}
                        onClick={() => command("retry", { jobId: j.id })}
                      >
                        Retry with the same command
                      </button>
                    )}
                  </div>
                ))}
                {data.invoices
                  .filter(
                    (i) =>
                      i.payment_state === "requires_action" ||
                      i.payment_state === "requires_payment_method" ||
                      (i.status === "open" &&
                        !!i.due_at &&
                        i.due_at < data.now),
                  )
                  .map((i) => (
                    <p key={i.id}>
                      {i.stripe_id} · {i.payment_state ?? "Past due"} ·
                      Open the invoice tab to pay or confirm the card
                    </p>
                  ))}
              </article>
              <article className={styles.card}>
                <h3>Change history</h3>
                {data.audit.map((a) => (
                  <div className={styles.row} key={a.id}>
                    <div>
                      <strong>{a.action}</strong>
                      <p>{a.reason}</p>
                      <span className={styles.muted}>
                        Performed by {a.actor}
                      </span>
                    </div>
                    <time>{time(a.created_at)}</time>
                  </div>
                ))}
              </article>
            </></FeatureWidget>}
        </>
      )}
    </WidgetGrid></section>
  );
}
