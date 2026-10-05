"use client";
import { Card } from "@/components/ui/foundation";

import { InlineNotice } from "@/components/ui/foundation";

import {
  ActionLink as UiActionLink,
  ActionAnchor as UiActionAnchor,
} from "@/components/ui/foundation";

import { Button } from "@/components/ui/foundation";

import { RecordLink as Link } from "@/components/records/record-link";
import { useEffect, useRef, useState, type ReactNode } from "react";
import { useRouter } from "next/navigation";
import { workFetch } from "@/lib/work-fetch";
import { payrollErrorMessage, type PayrollApiError } from "@/lib/payroll-errors";
import { useMoneyFormatter } from "@/components/money/currency-provider";
import styles from "./payroll-workspace.module.css";
type Row = Record<string, unknown>;
export function PayrollDisclosure({ title, children }: { title: string; children: ReactNode }) {
  return (
    <details className="ui-panel">
      <summary>{title}</summary>
      {children}
    </details>
  );
}
export function PayrollHeading({ title }: { title: string }) {
  return <h2>{title}</h2>;
}
export function PayrollNote({ text }: { text: string }) {
  return <p className="muted">{text}</p>;
}
export function PayrollAlert({ message }: { message: string }) {
  return (
    <InlineNotice tone="danger" className="notice" role="alert">
      {message}
    </InlineNotice>
  );
}
export function PayrollPageIntro({
  base,
  title,
  description,
}: {
  base: string;
  title: string;
  description: string;
}) {
  return (
    <header className={styles.header}>
      <div>
        <Link className="back-link" href={`${base}/people/payroll`}>
          ← Payroll
        </Link>
        <p className="eyebrow">Payroll</p>
        <h1>{title}</h1>
        <p className="muted">{description}</p>
      </div>
    </header>
  );
}
export function PayrollPolicySummary({
  base,
  run,
  exports,
}: {
  base: string;
  userId: string;
  run: Row;
  exports: Row[];
}) {
  return (
    <>
      <p>
        <strong>
          {"Overtime multiplier"} {Number(run.overtime_multiplier_bps) / 10000}×
        </strong>{" "}
        · {"Proration"} {String(run.proration_method).replaceAll("_", " ")} · {"Input revision"}{" "}
        {String(run.revision)}
      </p>
      <p className="muted">
        {run.country_pack_id && ["production","final_pay"].includes(String(run.mode))
          ? "Thai payroll uses the dated rule pack and independently approved employee evidence. Engineering calculations are checked; provider certification and bank delivery are reported separately."
          : "These Company settings support the payroll preview. Select a dated Thai rule pack and reviewed employee evidence for statutory calculation."}
      </p>
      {exports.map((item) => (
        <UiActionLink appearance="standard"
          variant="secondary"
          key={String(item.id)}
          href={`/api/v1${base}/people/payroll/runs/${run.id}/review-file?revision=${item.run_revision}`}
        >
          {"Review revision"} {String(item.run_revision)} · CSV
        </UiActionLink>
      ))}
    </>
  );
}
function date(value: unknown) {
  const raw = String(value ?? "");
  const parsed = /^\d{4}-\d{2}-\d{2}$/.test(raw) ? new Date(`${raw}T12:00:00Z`) : null;
  return parsed && !Number.isNaN(parsed.getTime())
    ? new Intl.DateTimeFormat("en-GB", {
        day: "numeric",
        month: "short",
        year: "numeric",
        timeZone: "UTC",
      }).format(parsed)
    : raw;
}
function sourceHref(
  base: string,
  blocker: {
    code: string;
    href?: string;
  },
): string | null {
  const href = blocker.href?.startsWith(`${base}/`)
    ? blocker.href
    : blocker.href?.startsWith("/people/")
      ? `${base}${blocker.href}`
      : null;
  return href && blocker.code === "COST_ASSIGNMENT_UNVERIFIED"
    ? `${href}#payroll-cost-assignments`
    : href;
}
function totalsOf(run: Row) {
  try {
    return JSON.parse(String(run.totals_json ?? "{}")) as Record<string, number>;
  } catch {
    return {} as Record<string, number>;
  }
}
function currentRun(runs: Row[]): Row | undefined {
  const byPeriod = (a: Row, b: Row) => String(a.period_end).localeCompare(String(b.period_end));
  const awaitingHandoff = runs.filter(
    (run) =>
      ["in_review", "approved"].includes(String(run.status)) ||
      (run.status === "posted" && run.payment_status !== "settled"),
  );
  if (awaitingHandoff.length) return awaitingHandoff.sort(byPeriod)[0];
  const preparing = runs.filter((run) =>
    ["draft", "calculating", "preview", "failed"].includes(String(run.status)),
  );
  if (preparing.length) return preparing.sort((a, b) => byPeriod(b, a))[0];
  return runs[0];
}
export function payrollStage(run: Row): 0 | 1 | 2 {
  const status = String(run.status);
  return [
    "posted",
    "payment_draft",
    "payment_approved",
    "payment_exported",
    "payment_partially_settled",
    "payment_settled",
  ].includes(status)
    ? 2
    : ["in_review", "approved"].includes(status)
      ? 1
      : 0;
}
function Status({ status }: { status: string }) {
  const labels: Record<string, string> = {
    draft: "Draft",
    calculating: "Calculating",
    preview: "Ready for review",
    failed: "Needs attention",
    in_review: "In review",
    approved: "Approved",
    posted: "Posted",
    payment_draft: "Payment draft",
    payment_approved: "Payment approved",
    payment_exported: "File exported",
    payment_partially_settled: "Partially settled",
    payment_settled: "Settled",
  };
  return <span className={styles.status}>{labels[status] ?? status.replaceAll("_", " ")}</span>;
}
function StageLine({ stage }: { stage: 0 | 1 | 2 }) {
  const names = ["Prepare", "Review and approve", "Post and pay"];
  return (
    <ol className={styles.stages} aria-label={"Payroll stages"}>
      {names.map((name, index) => (
        <li key={name} data-active={stage === index} data-complete={stage > index}>
          <span>{index + 1}</span>
          {name}
        </li>
      ))}
    </ol>
  );
}
export function PayrollDashboard({
  base,
  runs,
  readiness,
  canPrepare,
  redacted,
}: {
  base: string;
  userId: string;
  runs: Row[];
  readiness: Readiness | null;
  canPrepare: boolean;
  redacted: boolean;
}) {
  const format = useMoneyFormatter();
  const current = currentRun(runs);
  const amount = (value: unknown, currency: unknown) =>
    format(String(value ?? 0), String(currency ?? "THB"), String(current?.pay_date ?? ""), 2);
  const totals = current ? totalsOf(current) : {};
  const stage = current ? payrollStage(current) : 0;
  const status = current
    ? String(
        current.status === "posted" && current.payment_status
          ? `payment_${current.payment_status}`
          : current.status,
      )
    : "";
  const issue = status === "failed";
  return (
    <div className={styles.workspace}>
      <header className={styles.header}>
        <div>
          <p className="eyebrow">Payroll · {"Workspace"}</p>
          <h1>{"Payroll"}</h1>
          <p className="muted">{"One place for the current run, totals, and the next task."}</p>
        </div>
      </header>
      <p className={styles.safety} role="note">
        Thai statutory runs require dated rules and independently approved employee evidence.
        Gross previews and sandbox payment exports identify their calculation and delivery environment.
      </p>
      {!current ? (
        <section className={styles.hero}>
          <h2>{redacted ? "No posted payment batch yet" : "No payroll run yet"}</h2>
          <p>
            {redacted
              ? "Treasury sees runs after Finance posts payroll and prepares a payment batch."
              : "Set up a pay group and employee data before the first run."}
          </p>
          {canPrepare && (
            <UiActionLink appearance="standard"
              variant="primary"
              href={`${base}/people?panel=settings&section=pay-groups`}
            >
              {"Set up a pay group"}
            </UiActionLink>
          )}
        </section>
      ) : (
        <>
          <section className={styles.hero} aria-labelledby="current-payroll-title">
            <div className={styles.heroTop}>
              <div>
                <p className="eyebrow">{"Current pay run"}</p>
                <h2 id="current-payroll-title">
                  {date(current.period_start)} – {date(current.period_end)}
                </h2>
                <p className="muted">
                  {"Pay date"} {date(current.pay_date)} · {String(current.currency)} · {"Revision"}{" "}
                  {String(current.revision)}
                </p>
              </div>
              <Status status={status} />
            </div>
            <StageLine stage={stage} />
            <div className={styles.metrics}>
              {!redacted && (
                <>
                  <div>
                    <span>{"Gross pay"}</span>
                    <strong>
                      {totals.gross === undefined ? "—" : amount(totals.gross, current.currency)}
                    </strong>
                  </div>
                  <div>
                    <span>{"Deductions"}</span>
                    <strong>
                      {totals.deductions === undefined
                        ? "—"
                        : amount(totals.deductions, current.currency)}
                    </strong>
                  </div>
                </>
              )}
              <div>
                <span>{"Net payable"}</span>
                <strong>
                  {redacted
                    ? amount(current.net_payable_minor, current.currency)
                    : totals.net === undefined
                      ? "—"
                      : amount(totals.net, current.currency)}
                </strong>
              </div>
            </div>
            <UiActionLink appearance="standard"
              variant="primary"
              href={`${base}/people/payroll/runs/${encodeURIComponent(String(current.id))}`}
            >
              {"Open run and continue"}
            </UiActionLink>
          </section>
          <section className={styles.columns}>
            <Card as="article" appearance="standard" className={styles.card}>
              <h2>{"Needs attention"}</h2>
              {readiness?.blockers.length ? (
                <ul>
                  {readiness.blockers.slice(0, 4).map((blocker, index) => {
                    const href =
                      sourceHref(base, blocker) ??
                      `${base}/people/payroll/runs/${encodeURIComponent(String(current.id))}`;
                    return (
                      <li key={`${blocker.code}:${index}`}>
                        <Link href={href}>{blocker.message}</Link>
                      </li>
                    );
                  })}
                </ul>
              ) : issue ? (
                <p>
                  <Link
                    href={`${base}/people/payroll/runs/${encodeURIComponent(String(current.id))}`}
                  >
                    {"Calculation failed. Open the run and correct source data."}
                  </Link>
                </p>
              ) : (
                <p className="muted">{"Review items and source readiness before submission."}</p>
              )}
              {!redacted && (
                <>
                  <Link href={`${base}/people`}>{"Employee records"}</Link> ·{" "}
                  <Link href={`${base}/people/roster`}>{"Time and roster"}</Link>
                </>
              )}
            </Card>
            <Card as="article" appearance="standard" className={styles.card}>
              <h2>{"Ownership"}</h2>
              <p>
                {stage === 0
                  ? "HR prepares and submits"
                  : stage === 1
                    ? "Approver and Finance review"
                    : "Finance and Treasury post and pay"}
              </p>
              <p className="muted">{"Approval and payment retain separate authorized roles."}</p>
            </Card>
          </section>
        </>
      )}
    </div>
  );
}
export function PayrollHistoryContent({
  base,
  runs,
}: {
  base: string;
  userId: string;
  runs: Row[];
}) {
  return (
    <div className={styles.workspace}>
      <div className={styles.header}>
        <div>
          <Link className="back-link" href={`${base}/people/payroll`}>
            ← Payroll
          </Link>
          <p className="eyebrow">Payroll</p>
          <h1>{"Payroll history"}</h1>
          <p className="muted">{"Review prior totals, input revisions, and retained evidence."}</p>
        </div>
      </div>
      <Card as="section" appearance="standard" className={styles.card} aria-label={"Payroll runs"}>
        {runs.length ? (
          runs.map((run) => (
            <Link
              className="ui-record-item"
              href={`${base}/people/payroll/runs/${encodeURIComponent(String(run.id))}`}
              key={String(run.id)}
            >
              <span className="row-main">
                <strong>
                  {date(run.period_start)} – {date(run.period_end)}
                </strong>
                <small>
                  {String(run.currency)} · {"Pay date"} {date(run.pay_date)} · {"Revision"}{" "}
                  {String(run.revision ?? "—")}
                </small>
              </span>
              <Status
                status={String(
                  run.status === "posted" && run.payment_status
                    ? `payment_${run.payment_status}`
                    : run.status,
                )}
              />
            </Link>
          ))
        ) : (
          <p>{"No runs yet"}</p>
        )}
      </Card>
    </div>
  );
}
export function PayrollItemList({
  run,
  items,
  previousNet,
}: {
  userId: string;
  run: Row;
  items: Row[];
  previousNet: Record<string, number>;
}) {
  const format = useMoneyFormatter();
  const amount = (value: unknown, currency: unknown) =>
    format(String(value ?? 0), String(currency ?? "THB"), String(run.pay_date), 2);
  return (
    <Card as="section" appearance="standard"
      className={styles.card}
      id="payroll-record-evidence"
      aria-labelledby="payroll-items-heading"
    >
      <h2 id="payroll-items-heading">{run.country_pack_id && ["production","final_pay"].includes(String(run.mode)) ? "Employee payroll breakdown" : "Employee preview breakdown"}</h2>
      <p className="muted">
        {run.country_pack_id && ["production","final_pay"].includes(String(run.mode))
          ? "Calculated with the dated Thai rule pack and reviewed employee inputs. Posted employee payslips retain this revision; the payment section shows bank delivery separately."
          : "These figures support draft review. Issued employee payslips become available from the independently posted run; payment delivery is shown separately."}
      </p>
      <p className="muted">
        {date(run.period_start)} – {date(run.period_end)} · {"Pay date"} {date(run.pay_date)} ·{" "}
        {"Revision"} {String(run.revision)} · {String(run.currency)}
      </p>
      {items.length ? (
        items.map((item) => {
          const prior = previousNet[String(item.engagement_id)],
            net = Number(item.net_minor),
            delta = prior === undefined ? null : net - prior;
          return (
            <details className={styles.item} key={String(item.id)}>
              <summary>
                <strong>{String(item.legal_name)}</strong>
                <span>
                  {amount(net, item.currency)} {"net"}
                </span>
              </summary>
              <div className={styles.metrics}>
                <div>
                  <span>{"Gross pay"}</span>
                  <strong>{amount(item.gross_minor, item.currency)}</strong>
                </div>
                <div>
                  <span>{"Deductions"}</span>
                  <strong>{amount(item.deductions_minor, item.currency)}</strong>
                </div>
                <div>
                  <span>{"Employer contribution"}</span>
                  <strong>{amount(item.employer_cost_minor, item.currency)}</strong>
                </div>
                <div>
                  <span>{"Net pay"}</span>
                  <strong>{amount(item.net_minor, item.currency)}</strong>
                </div>
              </div>
              {prior !== undefined && (
                <p className="muted">
                  {"Compared with previous run"}: {delta !== null && delta >= 0 ? "+" : "−"}
                  {amount(Math.abs(delta ?? 0), item.currency)} ({"previous"}{" "}
                  {amount(prior, item.currency)})
                </p>
              )}
            </details>
          );
        })
      ) : (
        <p className="muted">{"No calculated employee items yet."}</p>
      )}
    </Card>
  );
}
export function PayrollDetailOverview({
  base,
  run,
  itemCount,
  netPayableMinor,
  paymentStatus,
  redacted,
}: {
  base: string;
  userId: string;
  run: Row;
  itemCount: number;
  netPayableMinor?: number;
  paymentStatus?: string;
  redacted: boolean;
}) {
  const format = useMoneyFormatter();
  const amount = (value: unknown, currency: unknown) =>
    format(String(value ?? 0), String(currency ?? "THB"), String(run.pay_date), 2);
  const totals = totalsOf(run);
  const stage = payrollStage(run);
  return (
    <div className={styles.workspace}>
      <div className={styles.header}>
        <div>
          <Link className="back-link" href={`${base}/people/payroll`}>
            ← {"Back to Payroll"}
          </Link>
          <p className="eyebrow">Payroll · {"Run detail"}</p>
          <h1>
            {date(run.period_start)} – {date(run.period_end)}
          </h1>
          <p className="muted">
            {"Pay date"} {date(run.pay_date)} · {String(run.currency)} · {"Revision"}{" "}
            {String(run.revision)}
          </p>
        </div>
      </div>
      <StageLine stage={stage} />
      <div className={styles.metrics}>
        {!redacted && (
          <>
            <div>
              <span>{"Gross pay"}</span>
              <strong>
                {totals.gross === undefined ? "—" : amount(totals.gross, run.currency)}
              </strong>
            </div>
            <div>
              <span>{"Deductions"}</span>
              <strong>
                {totals.deductions === undefined ? "—" : amount(totals.deductions, run.currency)}
              </strong>
            </div>
            <div>
              <span>{"Employer contribution"}</span>
              <strong>
                {totals.employer === undefined ? "—" : amount(totals.employer, run.currency)}
              </strong>
            </div>
          </>
        )}
        <div>
          <span>{"Net pay"}</span>
          <strong>
            {redacted
              ? amount(netPayableMinor, run.currency)
              : totals.net === undefined
                ? "—"
                : amount(totals.net, run.currency)}
          </strong>
        </div>
      </div>
      <Card as="section" appearance="standard" className={styles.card}>
        <div className={styles.heroTop}>
          <div>
            <h2>{"Next step"}</h2>
            <p>
              {stage === 0
                  ? `Review ${itemCount} people and source inputs before submission.`
                  : stage === 1
                    ? "Check totals and the approved input revision before Finance posting."
                    : "Review the journal and bank result evidence."}
            </p>
          </div>
          <Status
            status={
              run.status === "posted" && paymentStatus
                ? `payment_${paymentStatus}`
                : String(run.status)
            }
          />
        </div>
        {(
          <UiActionAnchor appearance="standard"
            variant="primary"
            href="#payroll-run-work"
          >
            Continue this stage
          </UiActionAnchor>
        )}
      </Card>
    </div>
  );
}
type Readiness = {
  runId: string;
  status: string;
  version: number;
  inputDigest: string | null;
  readyToCalculate: boolean;
  readyToSubmit: boolean;
  readyToApprove: boolean;
  sourceFresh: boolean;
  blockers: Array<{
    code: string;
    message: string;
    href?: string;
  }>;
};
export function PayrollReadinessPanel({
  base,
  runId,
  version,
  canPrepare,
  canApprove,
  canExport,
  simulationMode,
}: {
  base: string;
  runId: string;
  version: number;
  userId: string;
  canPrepare: boolean;
  canApprove: boolean;
  canExport: boolean;
  simulationMode: boolean;
}) {
  const router = useRouter();
  const [readiness, setReadiness] = useState<Readiness | null>(null);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const [showAllBlockers, setShowAllBlockers] = useState(false);
  const attempt = useRef<{
    body: string;
    key: string;
  } | null>(null);
  useEffect(() => {
    let active = true;
    void workFetch(`/api/v1${base}/people/payroll/runs/${encodeURIComponent(runId)}/readiness`, {
      cache: "no-store",
    })
      .then(async (response) => {
        const body = (await response.json()) as {
          data?: Readiness;
          error?: PayrollApiError;
        };
        if (!response.ok || !body.data)
          throw body.error ?? { message: "Readiness is unavailable." };
        if (active) setReadiness(body.data);
      })
      .catch((cause) => {
        if (active) setError(payrollErrorMessage(cause, "Readiness is unavailable."));
      });
    return () => {
      active = false;
    };
  }, [base, runId, version]);
  async function command(
    action: "calculate" | "submit-review" | "approve" | "export-review",
    input: unknown,
  ) {
    const bodyText = JSON.stringify(input),
      path = `payroll/runs/${encodeURIComponent(runId)}/${action}`;
    if (attempt.current?.body !== path + bodyText)
      attempt.current = { body: path + bodyText, key: crypto.randomUUID() };
    setBusy(true);
    setError("");
    try {
      const response = await workFetch(`/api/v1${base}/people/${path}`, {
        method: "POST",
        headers: { "Content-Type": "application/json", "Idempotency-Key": attempt.current.key },
        body: bodyText,
      });
      const body = (await response.json()) as {
        error?: PayrollApiError;
      };
      if (!response.ok) throw body.error ?? { message: "Could not complete this Payroll step." };
      attempt.current = null;
      router.refresh();
    } catch (cause) {
      setError(payrollErrorMessage(cause, "Could not complete this Payroll step."));
    } finally {
      setBusy(false);
    }
  }
  return (
    <Card as="section" appearance="standard" className={styles.card} aria-labelledby="payroll-readiness-title">
      <h2 id="payroll-readiness-title">{"Readiness and next action"}</h2>
      {error && <InlineNotice tone="danger" role="alert">{error}</InlineNotice>}
      {!readiness && !error && <p className="muted">{"Checking source data…"}</p>}
      {readiness && (
        <>
          <p>
            {readiness.sourceFresh
              ? "Source inputs match the preview"
              : "Source inputs changed; recalculate before review"}
          </p>
          {readiness.blockers.length ? (
            <>
              <ul>
                {(showAllBlockers ? readiness.blockers : readiness.blockers.slice(0, 5)).map(
                  (blocker, index) => {
                    const href = sourceHref(base, blocker);
                    return (
                      <li key={`${blocker.code}:${index}`}>
                        {blocker.message}
                        {href && (
                          <>
                            {" "}
                            · <Link href={href}>{"Open source"}</Link>
                          </>
                        )}
                      </li>
                    );
                  },
                )}
              </ul>
              {readiness.blockers.length > 5 && (
                <Button appearance="standard"
                  variant="secondary"
                  type="button"
                  aria-expanded={showAllBlockers}
                  onClick={() => setShowAllBlockers(!showAllBlockers)}
                >
                  {showAllBlockers
                    ? "Show fewer issues"
                    : `Show ${readiness.blockers.length - 5} more issues`}
                </Button>
              )}
            </>
          ) : (
            <p className="muted">{"No blocking issues"}</p>
          )}
          {canPrepare && ["draft", "calculating", "failed"].includes(readiness.status) && (
            <Button appearance="standard"
              variant="primary"
              type="button"
              disabled={busy || !readiness.readyToCalculate}
              onClick={() => void command("calculate", {})}
            >
              {simulationMode ? "Calculate mock preview" : "Calculate payroll preview"}
            </Button>
          )}
          {canPrepare && readiness.status === "preview" && (
            <div className="ui-form-actions">
              <Button appearance="standard"
                variant="primary"
                type="button"
                disabled={busy || !readiness.readyToSubmit || !readiness.inputDigest}
                onClick={() =>
                  void command("submit-review", {
                    expectedVersion: readiness.version,
                    inputDigest: readiness.inputDigest,
                  })
                }
              >
                {busy ? "Submitting…" : "Submit for review"}
              </Button>
              <Button appearance="standard"
                variant="secondary"
                type="button"
                disabled={busy || !readiness.readyToCalculate}
                onClick={() => void command("calculate", {})}
              >
                {"Recalculate"}
              </Button>
            </div>
          )}
          {readiness.status === "in_review" && (
            <p className="muted">{"Awaiting an independent approver."}</p>
          )}
          {canApprove && readiness.status === "in_review" && (
            <Button appearance="standard"
              variant="primary"
              type="button"
              disabled={busy || !readiness.readyToApprove}
              onClick={() => void command("approve", { expectedVersion: readiness.version })}
            >
              {"Approve submitted review"}
            </Button>
          )}
          {canExport && ["preview", "in_review"].includes(readiness.status) && (
            <Button appearance="standard"
              variant="secondary"
              type="button"
              disabled={busy}
              onClick={() => void command("export-review", {})}
            >
              {"Export review report"}
            </Button>
          )}
        </>
      )}
    </Card>
  );
}
