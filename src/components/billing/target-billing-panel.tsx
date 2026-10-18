"use client";
import { canonicalUiHref } from "@/lib/work_projection/destination-registry";
import {CollectionRecordLink,RecordCollectionDrawer,RecordFacts} from "@/components/records/record-collection";
import { InlineNotice } from "@/components/ui/foundation";

import { FormDisclosure } from "@/components/ui/form-disclosure";
import { Button, Field, Select, Table, TableFrame, TextInput } from "@/components/ui/foundation";

import { TimeText } from "@/components/people_payroll/time/time-preference-context";

import { betaFetch } from "@/lib/shared/beta-client";
import { billingMonth, monthBounds } from "@/lib/billing/billing";
import { workFetch } from "@/lib/work_projection/work-fetch";
import { RecordLink as Link } from "@/components/records/record-link";
import { useCallback, useEffect, useState } from "react";
import styles from "./target-billing-panel.module.css";

type TargetLine = {
  sourceKey?: string | null;
  kind: "base" | "storage" | "ai" | "seat";
  description: string;
  amountSatang: number | null;
  measuredQuantity: string | null;
  verifiedProviderCostSatang?: number | null;
  evidenceId?: string | null;
  statementId?: string | null;
  reconciliationStatus?: "verified" | "missing" | "not_required";
  provider?: string | null;
  fundingSource?: "erpx" | "byok" | "simulation" | "unknown" | null;
  requestStatus?: string | null;
  providerUnits?: Record<string, number> | null;
};
type Statement = {
  provider: string;
  statementId: string;
  originalCurrency: string;
  providerTotalMinor: string;
  reconciledTotalMinor: string;
  status: "verified" | "mismatch";
};
type MeasuredEvidence = {
  snapshot: {
    id?: string;
    refreshedAt: number;
    sourceCount: number;
    seatSourceCount: number;
    aiCoverage: "measured" | "partial";
    storageCoverage: "measured" | "partial";
    seatCoverage: "measured" | "unavailable";
  } | null;
  ai: {
    requests: number | null;
    inputTokens: number | null;
    outputTokens: number | null;
    platformSucceeded: number;
    platformVerified: number;
    platformPending: number;
    byok: number;
    failed: number;
    simulation: number;
    unknown: number;
    verifiedProviderCostSatang?: number | null;
    chargeableVerifiedProviderCostSatang?: number | null;
    totalProviderCostSatang?: number | null;
    sources?: Array<{
      sourceKey: string;
      provider: string | null;
      fundingSource: string;
      requestStatus: string | null;
      gatewayStatus: string | null;
      providerUnits: Record<string, number> | null;
      costStatus: string | null;
      verifiedProviderCostSatang: number | null;
      billableStatus: string;
    }>;
  };
  storage: {
    originalCount: number | null;
    activeBytesAtEnd: number | null;
    byteMilliseconds: string | null;
    coverage: "measured" | "partial" | "unavailable";
  };
  seats: {
    distinctUsers: number | null;
    activeSeatMilliseconds: string | null;
    coverage: "measured" | "unavailable";
    activeNow: { status: "measured" | "unavailable"; count: number | null; asOf: number | null };
  };
  operatingCosts?: {
    categories: Record<
      "infrastructure" | "backup" | "stripe" | "support",
      { status: "verified" | "unavailable"; amountSatang: number | null }
    >;
    totalVerifiedSatang: number | null;
  };
};
function billingLineIdentity(line: {sourceKey?:string|null;kind:string;description:string},index:number){return JSON.stringify([line.sourceKey??null,line.kind,line.description,index]);}
type TargetView = {
  holdingId: string;
  month: string;
  assignment: {
    mode: "shadow" | "sandbox_invoice";
    effectiveMonth: string;
    priceId: string;
    serviceStartedAt: number;
  } | null;
  price: {
    id: string;
    baseMonthlySatang: number;
    includedOriginalBytes: number;
    storageSatangPerGbMonth: number;
    aiMarkupPercent: number;
    seatTermsStatus: "undefined" | "defined";
    includedSeats: number | null;
    seatMonthlySatang: number | null;
    storageAllowanceMode: "holding_flat" | "per_paid_seat_time";
    ratingEnabled: boolean;
  } | null;
  measuredEvidence?: MeasuredEvidence;
  completeness: { ready: boolean; missing: string[]; meterSnapshotId: string | null };
  approval?: { eligible: boolean; blockers: string[] };
  lines: TargetLine[];
  sources?: TargetLine[];
  statementReconciliation?: Statement[];
  totalSatang: number | null;
  invoice: { id: string; totalSatang: number; status: string; stripeId: string | null } | null;
  state: "unassigned" | "held" | "ready" | "approved";
  measuredSourceCount?: number | null;
  verification?: { required: number; verified: number };
  financeProjection?: {
    invoiceSatang: number;
    receivableSatang: number;
    receivedSatang: number;
    reversedSatang: number;
    creditedSatang: number;
    stripeStatus: string;
    syncedAt: number;
  } | null;
};
type Invoice = { id: string; status: string; total_satang: number; stripe_id: string | null };
type GatewayPolicy = {
  platformEnabled: boolean;
  monthlyRequestLimit: number;
  monthlySpendCapSatang: number;
  reservationSatang: number;
  monthlyRequests: number;
  monthlyReservedOrVerifiedSatang: number;
  pendingPlatformCostRequests: number;
  updatedBy: string;
  updatedAt: number;
};

const amount = (satang: number | null | undefined) =>
  satang === null || satang === undefined
    ? "Pending review"
    : new Intl.NumberFormat("en-US", { style: "currency", currency: "THB" }).format(satang / 100);
const count = (value: number) => new Intl.NumberFormat("en-US").format(value);
function bytes(value: number | null | undefined) {
  if (value === null || value === undefined) return "Unavailable";
  if (value >= 1024 * 1024 * 1024)
    return `${(value / (1024 * 1024 * 1024)).toLocaleString("en-US", { maximumFractionDigits: 2 })} GiB`;
  if (value >= 1024 * 1024)
    return `${(value / (1024 * 1024)).toLocaleString("en-US", { maximumFractionDigits: 2 })} MiB`;
  return `${count(value)} bytes`;
}
function seatMonths(milliseconds: string | null | undefined, month: string) {
  if (milliseconds === null || milliseconds === undefined) return "Unavailable";
  const wholeMonth = BigInt(monthBounds(month).end - monthBounds(month).start);
  const thousandths = (BigInt(milliseconds) * BigInt(1000) + wholeMonth / BigInt(2)) / wholeMonth;
  return `${(Number(thousandths) / 1000).toLocaleString("en-US", { maximumFractionDigits: 3 })} seat-months`;
}
function storageGbMonths(byteMilliseconds: string | null | undefined, month: string) {
  if (byteMilliseconds === null || byteMilliseconds === undefined)
    return "Monthly byte-time unavailable";
  const wholeGbMonth =
    BigInt(monthBounds(month).end - monthBounds(month).start) * BigInt(1_000_000_000);
  const thousandths =
    (BigInt(byteMilliseconds) * BigInt(1000) + wholeGbMonth / BigInt(2)) / wholeGbMonth;
  return `${(Number(thousandths) / 1000).toLocaleString("en-US", { maximumFractionDigits: 3 })} GB-months measured`;
}
function observedAt(value: number | null) {
  return value === null ? "Observation time unavailable" : <TimeText appearance="standard" value={value} />;
}
const blockerNames: Record<string, string> = {
  target_offering_not_assigned: "Assign a target offering to this Holding.",
  price_version_unavailable: "The assigned price version is unavailable for this month.",
  month_not_closed: "Wait for the billing month to close.",
  meter_snapshot_missing: "A complete meter snapshot is missing.",
  ai_meter_incomplete: "AI measurement is incomplete.",
  storage_meter_incomplete: "Storage measurement or original-file reconciliation is incomplete.",
  meter_count_mismatch: "The measured source count does not match the snapshot.",
  storage_lifetime_missing: "An original file has no verified storage lifetime.",
  storage_lifetime_invalid: "An original file has an invalid storage lifetime.",
  ai_attribution_missing: "AI funding or source attribution is incomplete.",
  provider_cost_missing: "Verified provider cost evidence is missing for an AI source.",
  statement_unverified: "A provider statement has not been reconciled.",
  statement_mismatch: "Provider statement totals do not match verified costs.",
  statement_source_sum_mismatch: "Matched source costs do not add up to the provider statement.",
  storage_source_mismatch: "Original-file storage sources do not reconcile.",
  seat_terms_pending: "The assigned price version has no approved seat terms.",
  seat_rating_pending: "Seat charges have not been rated for this month.",
  seat_meter_pending: "The active named-seat meter is not complete.",
  seat_source_count_mismatch: "Named-seat sources do not match the sealed meter snapshot.",
  seat_intervals_invalid: "A named-seat activity interval is invalid or overlaps incorrectly.",
  price_version_provisional:
    "This historical price version is provisional and cannot issue an invoice.",
  shadow_only: "Shadow mode cannot issue an invoice.",
  sandbox_designation_missing: "This Holding is not designated for target Sandbox invoicing.",
  sandbox_designation_or_rollout_approval_missing:
    "This Holding needs Sandbox designation or rollout approval before invoicing.",
  ai_gateway_attribution_missing: "The AI gateway result or funding attribution is incomplete.",
  grandfathered_support_active: "The grandfathered Support plan overlaps this month.",
  month_already_invoiced: "An invoice already exists for this month.",
  automatic_payment_consent_missing: "Auto-pay requires a saved card and consent.",
  already_approved: "This target month was already approved.",
};
function blockerLabel(value: string) {
  const [code, source] = value.split(":", 2);
  const label = blockerNames[code] ?? value.replaceAll("_", " ");
  return source ? `${label} Source: ${source}.` : label;
}
function ratedLineAmount(line: TargetLine) {
  if ((line.kind === "storage" || line.kind === "seat") && line.sourceKey)
    return `Included in ${line.kind} subtotal`;
  return amount(line.amountSatang);
}
function providerCostLabel(line: TargetLine) {
  if (line.kind !== "ai" || line.reconciliationStatus === "not_required") return "Not applicable";
  return line.verifiedProviderCostSatang === null || line.verifiedProviderCostSatang === undefined
    ? "Pending verification"
    : amount(line.verifiedProviderCostSatang);
}
function measuredQuantityLabel(line: TargetLine) {
  if (line.measuredQuantity === null) return "—";
  if (line.kind === "base") return `${line.measuredQuantity} service days`;
  if (line.kind === "storage") return `${line.measuredQuantity} byte-ms`;
  if (line.kind === "seat") {
    const fraction = line.measuredQuantity.match(/^(\d+)\/(\d+) seat-months$/);
    if (fraction && BigInt(fraction[2]) > BigInt(0)) {
      const thousandths =
        (BigInt(fraction[1]) * BigInt(1000) + BigInt(fraction[2]) / BigInt(2)) /
        BigInt(fraction[2]);
      return `${(Number(thousandths) / 1000).toLocaleString("en-US", { maximumFractionDigits: 3 })} seat-months`;
    }
    return line.measuredQuantity;
  }
  return line.measuredQuantity;
}
function providerUnitsLabel(line: TargetLine) {
  if (line.kind !== "ai" || !line.providerUnits || !Object.keys(line.providerUnits).length)
    return "—";
  return Object.entries(line.providerUnits)
    .map(([unit, value]) => `${count(value)} ${unit.replaceAll("_", " ")}`)
    .join(" · ");
}

export function TargetBillingPanel({
  base,
  platform,
  holdingId,
  now,
  writable,
  reason,
  reasonValid,
  invoices,
  onCommand,
}: {
  base: string;
  platform: boolean;
  holdingId: string;
  now: number;
  writable: boolean;
  reason: string;
  reasonValid: boolean;
  invoices: Invoice[];
  onCommand?: (action: string, fields: Record<string, unknown>) => Promise<boolean>;
}) {
  const [month, setMonth] = useState(() => billingMonth(now));
  const [mode, setMode] = useState<"shadow" | "sandbox_invoice">("shadow");
  const [view, setView] = useState<TargetView | null>(null);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const [uploadBusy, setUploadBusy] = useState(false);
  const [uploadedEvidence, setUploadedEvidence] = useState<{ id: string; state: string } | null>(
    null,
  );
  const [selectedSourceKey, setSelectedSourceKey] = useState("");
  const [selectedProvider, setSelectedProvider] = useState("");
  const [gateway, setGateway] = useState<GatewayPolicy | null>(null);
  const [gatewayError, setGatewayError] = useState("");
  const [gatewayBusy, setGatewayBusy] = useState(false);
  const api = platform
    ? `/api/v1/platform/billing/target?holdingId=${encodeURIComponent(holdingId)}&month=${month}`
    : `/api/v1${base}/billing/target?month=${month}`;
  const refresh = useCallback(async () => {
    const result = await betaFetch<TargetView>(api);
    setView(result);
    setMode(result.assignment?.mode ?? "shadow");
    const first = result.sources?.find(
      (line) => line.kind === "ai" && line.sourceKey && line.reconciliationStatus === "missing",
    );
    setSelectedSourceKey(first?.sourceKey ?? "");
    setSelectedProvider(first?.provider ?? "");
  }, [api]);
  const gatewayApi = `/api/v1/platform/billing/ai-gateway?holdingId=${encodeURIComponent(holdingId)}`;
  const refreshGateway = useCallback(async () => {
    setGateway(await betaFetch<GatewayPolicy>(gatewayApi));
  }, [gatewayApi]);
  useEffect(() => {
    if (!platform) return;
    let active = true;
    betaFetch<GatewayPolicy>(gatewayApi)
      .then((result) => {
        if (active) {
          setGateway(result);
          setGatewayError("");
        }
      })
      .catch((failure) => {
        if (active)
          setGatewayError(
            failure instanceof Error ? failure.message : "AI gateway policy could not be loaded.",
          );
      });
    return () => {
      active = false;
    };
  }, [gatewayApi, platform]);
  useEffect(() => {
    let active = true;
    betaFetch<TargetView>(api)
      .then((result) => {
        if (active) {
          setView(result);
          setMode(result.assignment?.mode ?? "shadow");
          const first = result.sources?.find(
            (line) =>
              line.kind === "ai" && line.sourceKey && line.reconciliationStatus === "missing",
          );
          setSelectedSourceKey(first?.sourceKey ?? "");
          setSelectedProvider(first?.provider ?? "");
        }
      })
      .catch((reason) => {
        if (active)
          setError(
            reason instanceof Error ? reason.message : "Target billing could not be loaded.",
          );
      });
    return () => {
      active = false;
    };
  }, [api]);
  async function submit(action: string, fields: Record<string, unknown>) {
    if (!onCommand || !writable || !reasonValid || busy) return;
    setBusy(true);
    setError("");
    try {
      const saved = await onCommand(action, { targetMonth: month, ...fields });
      if (saved) await refresh();
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : "Please retry.");
    } finally {
      setBusy(false);
    }
  }
  async function uploadEvidence(file: File) {
    if (!platform || !writable) return;
    setUploadBusy(true);
    setError("");
    try {
      const response = await workFetch(
        `/api/v1/platform/billing/evidence?holdingId=${encodeURIComponent(holdingId)}`,
        {
          method: "POST",
          headers: {
            "Content-Type": file.type,
            "X-Filename": encodeURIComponent(file.name),
            "Idempotency-Key": crypto.randomUUID(),
          },
          body: file,
        },
      );
      const body = (await response.json()) as {
        data?: { id: string; state: string };
        error?: { message: string };
      };
      if (!response.ok || !body.data)
        throw new Error(body.error?.message ?? "Evidence upload failed.");
      setUploadedEvidence(body.data);
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : "Evidence upload failed.");
    } finally {
      setUploadBusy(false);
    }
  }
  async function changeGateway(enabled: boolean) {
    if (!gateway || !writable || !reasonValid || gatewayBusy) return;
    setGatewayBusy(true);
    setGatewayError("");
    try {
      const next = await betaFetch<GatewayPolicy>("/api/v1/platform/billing/ai-gateway", {
        holdingId,
        enabled,
        expectedUpdatedAt: gateway.updatedAt,
        reason: reason.trim(),
      });
      setGateway(next);
    } catch (failure) {
      setGatewayError(
        failure instanceof Error ? failure.message : "AI gateway policy could not be changed.",
      );
      await refreshGateway().catch(() => {});
    } finally {
      setGatewayBusy(false);
    }
  }
  const invoice = invoices.find((entry) => entry.id === view?.invoice?.id);
  const stripeId = view?.invoice?.stripeId ?? invoice?.stripe_id;
  const invoiceStatus = view?.invoice?.status ?? invoice?.status;
  const issued = stripeId && invoiceStatus !== "void";
  const evidence = view?.measuredEvidence;
  const measuredSources =
    evidence?.snapshot?.sourceCount ??
    view?.measuredSourceCount ??
    (view?.completeness.meterSnapshotId ? view.sources?.length : null);
  const verification =
    view?.verification ??
    (view?.sources
      ? {
          required: view.sources.filter(
            (line) => line.kind === "ai" && line.reconciliationStatus !== "not_required",
          ).length,
          verified: view.sources.filter(
            (line) => line.kind === "ai" && line.reconciliationStatus === "verified",
          ).length,
        }
      : null);
  const pendingAiSources =
    view?.sources?.filter(
      (line) => line.kind === "ai" && line.sourceKey && line.reconciliationStatus === "missing",
    ) ?? [];
  const additionalApprovalBlockers =
    view?.approval?.blockers.filter((item) => !view.completeness.missing.includes(item)) ?? [];
  const usageHref = platform
    ? canonicalUiHref(`${base}/manage-erpx/billing/usage?holdingId=${encodeURIComponent(holdingId)}&month=${month}`)
    : canonicalUiHref(`${base}/usage?month=${month}`);
  return (
    <section className={styles.root} aria-label="Target billing model">
      <div className={styles.heading}>
        <div>
          <h3>Target billing model</h3>
          <p>
            Measured seats and usage, verified provider cost, rated charge, and issued invoice are
            separate stages.
          </p>
        </div>
        <div className={styles.headingActions}>
          <Field appearance="standard">
            Month{" "}
            <TextInput appearance="standard"
              type="month"
              value={month}
              onChange={(event) => {
                const next = event.target.value;
                if (!/^\d{4}-(0[1-9]|1[0-2])$/.test(next)) return;
                setMonth(next);
                setView(null);
                setError("");
              }}
            />
          </Field>
          <Button appearance="standard"
            variant="ghost"
            type="button"
            onClick={() => {
              setError("");
              void refresh().catch((reason) =>
                setError(reason instanceof Error ? reason.message : "Please retry."),
              );
            }}
          >
            Refresh review
          </Button>
        </div>
      </div>
      {error && (
        <InlineNotice tone="danger" role="alert" className={styles.error}>
          {error}
        </InlineNotice>
      )}
      <RecordCollectionDrawer<TargetView> kind="billing_source" title="Measured billing source" endpoint={api} select={(snapshot,id)=>(snapshot.measuredEvidence?.ai.sources?.find(row=>row.sourceKey===id)??snapshot.sources?.find(row=>row.sourceKey===id)) as unknown as Record<string,unknown>??null} render={record=><RecordFacts record={record} fields={['sourceKey','provider','fundingSource','requestStatus','gatewayStatus','kind','description','measuredQuantity','verifiedProviderCostSatang','amountSatang','reconciliationStatus','statementId','evidenceId']}/>}/>
      <RecordCollectionDrawer<TargetView> kind="billing_statement" title="Provider reconciliation" endpoint={api} select={(snapshot,id)=>snapshot.statementReconciliation?.find(row=>row.statementId===id) as unknown as Record<string,unknown>??null} render={record=><RecordFacts record={record} fields={['provider','statementId','originalCurrency','providerTotalMinor','reconciledTotalMinor','status']}/>}/>
      <RecordCollectionDrawer<TargetView> kind="billing_line" title="Rated billing line" endpoint={api} select={(snapshot,id)=>snapshot.lines.find((row,index)=>billingLineIdentity(row,index)===id) as unknown as Record<string,unknown>??null} render={(record,snapshot)=><><RecordFacts record={record} fields={['kind','description','measuredQuantity','amountSatang','verifiedProviderCostSatang','provider','fundingSource','requestStatus','statementId','evidenceId','reconciliationStatus']}/><h2>Source records</h2>{snapshot.sources?.filter(row=>row.kind===record.kind&&(!record.sourceKey||row.sourceKey===record.sourceKey)).map((row,index)=><p key={row.sourceKey??index}>{row.sourceKey?<CollectionRecordLink kind="billing_source" id={row.sourceKey}>{row.description}</CollectionRecordLink>:row.description}</p>)}</>}/>
      {!error && !view && <InlineNotice tone="info" role="status">Loading target billing review…</InlineNotice>}
      {view && (
        <>
          <p className={styles.mode}>
            Mode:{" "}
            <strong>
              {view.assignment?.mode === "sandbox_invoice"
                ? "Sandbox invoice"
                : view.assignment?.mode === "shadow"
                  ? "Shadow rating"
                  : "Not assigned"}
            </strong>
            {view.assignment?.effectiveMonth && ` · Effective ${view.assignment.effectiveMonth}`}
            {view.assignment && !view.completeness.ready && " · Review incomplete"}
          </p>
          <section className={styles.evidenceSection} aria-label="Measured billing evidence">
            <div className={styles.evidenceHeading}>
              <div>
                <h4>Measured activity</h4>
                <p>
                  Actual meter readings for {view.month}. These quantities are separate from
                  verified costs, rated charges, and invoices.
                </p>
              </div>
              <span>
                {evidence?.snapshot ? (
                  <>Snapshot refreshed {observedAt(evidence.snapshot.refreshedAt)}</>
                ) : (
                  "Monthly snapshot unavailable"
                )}
              </span>
            </div>
            <div className={styles.evidenceGrid}>
              <article className={styles.evidenceCard}>
                <span>Active named seats now</span>
                <strong>
                  {evidence?.seats.activeNow.status === "measured" &&
                  evidence.seats.activeNow.count !== null
                    ? count(evidence.seats.activeNow.count)
                    : "Unavailable"}
                </strong>
                <small>
                  Live count as of{" "}
                  {evidence?.seats.activeNow
                    ? observedAt(evidence.seats.activeNow.asOf)
                    : "an unavailable observation"}
                  . This is not the month&apos;s billable seat-time.
                </small>
              </article>
              <article className={styles.evidenceCard}>
                <span>Seat-time in {view.month}</span>
                <strong>
                  {evidence?.seats.coverage === "measured"
                    ? seatMonths(evidence.seats.activeSeatMilliseconds, view.month)
                    : "Not measured"}
                </strong>
                <small>
                  {evidence?.seats.distinctUsers !== null &&
                  evidence?.seats.distinctUsers !== undefined
                    ? `${count(evidence.seats.distinctUsers)} distinct named seats observed`
                    : "Distinct-seat count unavailable"}{" "}
                  · {evidence?.seats.coverage ?? "unavailable"} coverage
                </small>
              </article>
              <article className={styles.evidenceCard}>
                <span>AI requests in {view.month}</span>
                <strong>
                  {evidence?.ai.requests === null || evidence?.ai.requests === undefined
                    ? "Not measured"
                    : count(evidence.ai.requests)}
                </strong>
                <small>
                  {evidence?.ai.inputTokens === null || evidence?.ai.inputTokens === undefined
                    ? "Input tokens unavailable"
                    : `${count(evidence.ai.inputTokens)} input tokens`}{" "}
                  ·{" "}
                  {evidence?.ai.outputTokens === null || evidence?.ai.outputTokens === undefined
                    ? "Output tokens unavailable"
                    : `${count(evidence.ai.outputTokens)} output tokens`}
                  . Provider-specific units may also apply.
                </small>
              </article>
              <article className={styles.evidenceCard}>
                <span>Active original files in snapshot</span>
                <strong>
                  {evidence?.storage.activeBytesAtEnd === null ||
                  evidence?.storage.activeBytesAtEnd === undefined
                    ? "Not measured"
                    : bytes(evidence.storage.activeBytesAtEnd)}
                </strong>
                <small>
                  {evidence?.storage.originalCount === null ||
                  evidence?.storage.originalCount === undefined
                    ? "File count unavailable"
                    : `${count(evidence.storage.originalCount)} originals recorded`}{" "}
                  · {evidence?.storage.coverage ?? "unavailable"} coverage ·{" "}
                  {storageGbMonths(evidence?.storage.byteMilliseconds, view.month)}
                </small>
              </article>
              {platform && (
                <article className={styles.evidenceCard}>
                  <span>Verified ERPX provider expense</span>
                  <strong>
                    {!evidence ||
                    evidence.ai.verifiedProviderCostSatang === null ||
                    evidence.ai.verifiedProviderCostSatang === undefined
                      ? "Unavailable"
                      : evidence.ai.verifiedProviderCostSatang === 0
                        ? "None verified"
                        : amount(evidence.ai.verifiedProviderCostSatang)}
                  </strong>
                  <small>
                    {evidence
                      ? `${count(evidence.ai.platformVerified)} successful requests verified · ${count(evidence.ai.platformPending)} pending`
                      : "Provider evidence unavailable"}
                    .{" "}
                    {evidence?.ai.chargeableVerifiedProviderCostSatang === null ||
                    evidence?.ai.chargeableVerifiedProviderCostSatang === undefined
                      ? "Chargeable successful-request cost unavailable."
                      : `Chargeable successful-request cost ${amount(evidence.ai.chargeableVerifiedProviderCostSatang)}.`}{" "}
                    {evidence?.ai.totalProviderCostSatang === null ||
                    evidence?.ai.totalProviderCostSatang === undefined
                      ? "Final provider total unavailable."
                      : `Final provider total ${amount(evidence.ai.totalProviderCostSatang)}.`}
                  </small>
                </article>
              )}
              {platform && (
                <article className={styles.evidenceCard}>
                  <span>Verified operating costs</span>
                  <strong>
                    {!evidence?.operatingCosts ||
                    !Object.values(evidence.operatingCosts.categories).some(
                      (entry) => entry.status === "verified",
                    )
                      ? "Unavailable"
                      : evidence.operatingCosts.totalVerifiedSatang === null
                        ? "Partial evidence"
                        : amount(evidence.operatingCosts.totalVerifiedSatang)}
                  </strong>
                  <small>
                    {evidence?.operatingCosts
                      ? Object.entries(evidence.operatingCosts.categories)
                          .map(
                            ([name, entry]) =>
                              `${name}: ${entry.status === "verified" ? amount(entry.amountSatang) : "unavailable"}`,
                          )
                          .join(" · ")
                      : "Infrastructure, backup, Stripe, and Support evidence unavailable"}
                    . Verified subtotal is not a complete cost total.
                  </small>
                </article>
              )}
            </div>
            {evidence?.snapshot && (
              <p className={styles.evidenceBreakdown}>
                AI source mix: {count(evidence.ai.byok)} BYOK ·{" "}
                {count(evidence.ai.platformSucceeded)} Platform-paid succeeded ·{" "}
                {count(evidence.ai.failed)} failed · {count(evidence.ai.simulation)} simulation ·{" "}
                {count(evidence.ai.unknown)} unknown. Partial coverage means these are measured
                records, not a complete monthly total.
              </p>
            )}
            {platform && evidence?.ai.sources?.length ? (
              <details className={styles.sourceDetails}>
                <summary>Review {count(evidence.ai.sources.length)} measured AI sources</summary>
                <TableFrame appearance="standard" label="Records" className={styles.scroll}>
                  <Table className="business-table">
                    <thead>
                      <tr>
                        <th>Source</th>
                        <th>Provider</th>
                        <th>Funding / result</th>
                        <th>Provider units</th>
                        <th>Cost status</th>
                        <th>Verified cost</th>
                      </tr>
                    </thead>
                    <tbody>
                      {evidence.ai.sources.map((source) => (
                        <tr key={source.sourceKey}>
                          <td><CollectionRecordLink kind="billing_source" id={source.sourceKey}>{source.sourceKey}</CollectionRecordLink></td>
                          <td>{source.provider ?? "Unknown"}</td>
                          <td>
                            {source.fundingSource} ·{" "}
                            {source.requestStatus ?? source.gatewayStatus ?? "Unknown"}
                          </td>
                          <td>
                            {providerUnitsLabel({
                              kind: "ai",
                              description: "",
                              amountSatang: null,
                              measuredQuantity: null,
                              providerUnits: source.providerUnits,
                            })}
                          </td>
                          <td>{source.billableStatus.replaceAll("_", " ")}</td>
                          <td>
                            {source.verifiedProviderCostSatang === null
                              ? "Unavailable"
                              : amount(source.verifiedProviderCostSatang)}
                          </td>
                        </tr>
                      ))}
                    </tbody>
                  </Table>
                </TableFrame>
              </details>
            ) : null}
          </section>
          {view.price && (
            <p className={styles.terms}>
              {view.price.ratingEnabled
                ? "Seat-only target Sandbox hypothesis"
                : "Historical provisional Sandbox price (not the current offer)"}{" "}
              version {view.price.id}:{" "}
              {view.price.baseMonthlySatang === 0
                ? "No Holding base fee"
                : `${amount(view.price.baseMonthlySatang)} proposed Holding base`}{" "}
              ·{" "}
              {view.price.storageAllowanceMode === "per_paid_seat_time"
                ? `${count(view.price.includedOriginalBytes / 1_000_000_000)} GB original-file allowance per full seat-month, scaled by paid seat-time`
                : `${count(view.price.includedOriginalBytes / 1_000_000_000)} GB proposed original-file allowance`}{" "}
              · {amount(view.price.storageSatangPerGbMonth)} per excess GB-month · verified
              Platform-paid AI cost plus {view.price.aiMarkupPercent}%.
              {view.price.seatTermsStatus === "defined" && view.price.seatMonthlySatang !== null
                ? ` ${amount(view.price.seatMonthlySatang)} per active named seat-month from the first seat; ${view.price.includedSeats ?? 0} seats included.`
                : " Seat terms are not defined for this price version; it cannot be invoiced."}{" "}
              This is a Sandbox price hypothesis, not a live storage entitlement or collection
              commitment.
            </p>
          )}
          <div className={styles.stages} aria-label="Billing stages">
            <article>
              <span>Meter snapshot</span>
              <strong>
                {measuredSources === null || measuredSources === undefined
                  ? "Unavailable"
                  : `${count(measuredSources)} source groups`}
              </strong>
              <Link href={usageHref}>View measured usage</Link>
            </article>
            <article>
              <span>Verified provider expense</span>
              <strong>
                {!platform
                  ? "ERPX review"
                  : evidence?.ai.verifiedProviderCostSatang !== null &&
                      evidence?.ai.verifiedProviderCostSatang !== undefined
                    ? evidence.ai.verifiedProviderCostSatang === 0
                      ? "None verified"
                      : amount(evidence.ai.verifiedProviderCostSatang)
                    : verification && view.completeness.meterSnapshotId
                      ? `${count(verification.verified)} / ${count(verification.required)} AI sources`
                      : "Pending verification"}
              </strong>
              <small>Expense is separate from chargeable AI cost</small>
            </article>
            <article>
              <span>Rated charge</span>
              <strong>{view.assignment ? amount(view.totalSatang) : "Not rated"}</strong>
              <small>
                {!view.assignment
                  ? "Target offering is not assigned"
                  : view.assignment.mode === "shadow"
                    ? "Shadow only · No invoice"
                    : "Before invoice approval"}
              </small>
            </article>
            <article>
              <span>Issued Stripe amount</span>
              <strong>
                {issued
                  ? amount(view.invoice?.totalSatang ?? invoice?.total_satang)
                  : view.invoice
                    ? "Stripe issuance pending"
                    : "No issued invoice"}
              </strong>
              <small>
                {issued
                  ? invoiceStatus
                  : view.invoice
                    ? "Approved internally; awaiting Stripe"
                    : "Invoice approval is separate"}
              </small>
            </article>
          </div>
          {platform && (
            <section className={styles.review} aria-label="Platform AI gateway policy">
              <h4>Platform-funded AI control</h4>
              {gatewayError && (
                <InlineNotice tone="danger" role="alert" className={styles.error}>
                  {gatewayError}
                </InlineNotice>
              )}
              {gateway ? (
                <>
                  <p>
                    Platform key: <strong>{gateway.platformEnabled ? "Enabled" : "Paused"}</strong>{" "}
                    · {count(gateway.monthlyRequests)} of {count(gateway.monthlyRequestLimit)}{" "}
                    monthly requests · {amount(gateway.monthlyReservedOrVerifiedSatang)} reservation
                    or verified cost against a {amount(gateway.monthlySpendCapSatang)} monthly cap.
                  </p>
                  <p>
                    {count(gateway.pendingPlatformCostRequests)} Platform-paid requests still need
                    cost reconciliation. Reservation figures may differ from final provider charges.
                  </p>
                  <p className={styles.subtle}>
                    Last changed by {gateway.updatedBy} · <TimeText appearance="standard" value={gateway.updatedAt} />
                  </p>
                  {writable && (
                    <Button appearance="standard"
                      variant="ghost"
                      type="button"
                      disabled={gatewayBusy || !reasonValid}
                      onClick={() => void changeGateway(!gateway.platformEnabled)}
                    >
                      {gatewayBusy
                        ? "Saving…"
                        : gateway.platformEnabled
                          ? "Pause Platform key for this Holding"
                          : "Resume Platform key for this Holding"}
                    </Button>
                  )}
                  {writable && (
                    <p>Enter the action reason above before changing the gateway policy.</p>
                  )}
                </>
              ) : (
                !gatewayError && <p>Loading AI gateway policy…</p>
              )}
            </section>
          )}
          {platform && (
            <section className={styles.review} aria-label="Stripe and Finance reconciliation">
              <h4>Stripe and Finance reconciliation</h4>
              {!view.invoice ? (
                <p>No target invoice has been approved for this month.</p>
              ) : !view.financeProjection ? (
                <p>
                  Target invoice {view.invoice.id} is recorded. Finance delivery has not been
                  confirmed yet.
                </p>
              ) : (
                <>
                  <p>
                    Stripe status: <strong>{view.financeProjection.stripeStatus}</strong> · Finance
                    delivery last synced <TimeText appearance="standard" value={view.financeProjection.syncedAt} />
                  </p>
                  {view.financeProjection.invoiceSatang !== view.invoice.totalSatang && (
                    <InlineNotice tone="danger" role="alert" className={styles.error}>
                      Finance invoice amount does not match the approved target invoice.
                    </InlineNotice>
                  )}
                  <div className={styles.financeAmounts}>
                    <span>
                      Invoice <strong>{amount(view.financeProjection.invoiceSatang)}</strong>
                    </span>
                    <span>
                      Receivable <strong>{amount(view.financeProjection.receivableSatang)}</strong>
                    </span>
                    <span>
                      Received <strong>{amount(view.financeProjection.receivedSatang)}</strong>
                    </span>
                    <span>
                      Reversed <strong>{amount(view.financeProjection.reversedSatang)}</strong>
                    </span>
                    <span>
                      Credited <strong>{amount(view.financeProjection.creditedSatang)}</strong>
                    </span>
                  </div>
                </>
              )}
            </section>
          )}
          <section className={styles.review}>
            <h4>Completeness and reconciliation</h4>
            {view.completeness.ready ? (
              <p>Ready for review. All required rating inputs are present.</p>
            ) : (
              <>
                <p>Approval is blocked until these items are resolved:</p>
                <ul>
                  {view.completeness.missing.map((item, index) => (
                    <li key={`${item}:${index}`}>{blockerLabel(item)}</li>
                  ))}
                </ul>
              </>
            )}
            {platform && additionalApprovalBlockers.length ? (
              <>
                <p>Invoice approval also requires:</p>
                <ul>
                  {additionalApprovalBlockers.map((item, index) => (
                    <li key={`${item}:${index}`}>{blockerLabel(item)}</li>
                  ))}
                </ul>
              </>
            ) : null}
            {view.price && !view.price.ratingEnabled && (
              <p>
                This historical or incomplete price version cannot issue a target Sandbox invoice.
                Assign the approved seat-only version for a future month.
              </p>
            )}
            {platform && view.statementReconciliation?.length ? (
              <TableFrame appearance="standard" label="Records" className={styles.scroll}>
                <Table className="business-table">
                  <thead>
                    <tr>
                      <th>Provider</th>
                      <th>Statement</th>
                      <th>Reconciliation</th>
                      <th>Provider amount</th>
                      <th>Reconciled source amount</th>
                    </tr>
                  </thead>
                  <tbody>
                    {view.statementReconciliation.map((statement) => (
                      <tr key={`${statement.provider}:${statement.statementId}`}>
                        <td>{statement.provider}</td>
                        <td><CollectionRecordLink kind="billing_statement" id={statement.statementId}>{statement.statementId}</CollectionRecordLink></td>
                        <td>{statement.status}</td>
                        <td>
                          {statement.providerTotalMinor} {statement.originalCurrency} minor units
                        </td>
                        <td>
                          {statement.reconciledTotalMinor} {statement.originalCurrency} minor units
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </Table>
              </TableFrame>
            ) : platform ? (
              <p>No reconciled provider statements for this month.</p>
            ) : (
              <p>
                ERPX reviews provider evidence and statement reconciliation before rating a charge.
              </p>
            )}
          </section>
          <section className={styles.review}>
            <h4>Rated lines</h4>
            <TableFrame appearance="standard" label="Records" className={styles.scroll}>
              <Table className="business-table">
                <thead>
                  <tr>
                    <th>Type</th>
                    <th>Description</th>
                    <th>Measured quantity</th>
                    {platform && (
                      <>
                        <th>Provider units</th>
                        <th>Provider / funding</th>
                        <th>Verified provider cost</th>
                      </>
                    )}
                    <th>Rated charge</th>
                  </tr>
                </thead>
                <tbody>
                  {view.lines.map((line, index) => (
                    <tr key={`${line.sourceKey ?? line.kind}:${index}`}>
                      <td>{line.kind}</td>
                      <td><CollectionRecordLink kind="billing_line" id={billingLineIdentity(line,index)}>{line.description}</CollectionRecordLink></td>
                      <td>{measuredQuantityLabel(line)}</td>
                      {platform && (
                        <>
                          <td>{providerUnitsLabel(line)}</td>
                          <td>
                            {line.provider ?? "—"}
                            {line.fundingSource ? ` · ${line.fundingSource}` : ""}
                            {line.requestStatus ? ` · ${line.requestStatus}` : ""}
                          </td>
                          <td>{providerCostLabel(line)}</td>
                        </>
                      )}
                      <td>{ratedLineAmount(line)}</td>
                    </tr>
                  ))}
                </tbody>
              </Table>
            </TableFrame>
            {!view.lines.length && <p>No rated lines are available yet.</p>}
          </section>
          {platform && writable && (
            <div className={styles.actions}>
              <h4>Platform review</h4>
              <p>
                Enter an action reason above before saving. Rating cannot issue an invoice until all
                checks pass.
              </p>
              <div className={styles.evidenceUpload}>
                <Field appearance="standard">
                  Private review evidence (PDF, PNG, or JPEG; up to 10 MiB)
                  <TextInput appearance="standard"
                    type="file"
                    accept="application/pdf,image/png,image/jpeg"
                    disabled={uploadBusy}
                    onChange={(event) => {
                      const file = event.target.files?.[0];
                      if (file) void uploadEvidence(file);
                    }}
                  />
                </Field>
                {uploadBusy && <span>Uploading evidence…</span>}
                {uploadedEvidence && (
                  <InlineNotice tone="info" role="status">
                    Evidence ID: <code>{uploadedEvidence.id}</code> · {uploadedEvidence.state}.
                    Verification requires the security scan to mark this file available.
                  </InlineNotice>
                )}
              </div>
              <div className={styles.formRow}>
                <Field appearance="standard">
                  Target mode{" "}
                  <Select appearance="standard"
                    value={mode}
                    onChange={(event) =>
                      setMode(event.target.value as "shadow" | "sandbox_invoice")
                    }
                  >
                    <option value="shadow">Shadow rating</option>
                    <option value="sandbox_invoice">Sandbox invoice</option>
                  </Select>
                </Field>
                <Button appearance="standard"
                  variant="ghost"
                  type="button"
                  disabled={busy || !reasonValid || view.assignment?.effectiveMonth === month}
                  onClick={() => submit("assign-target", { targetMode: mode })}
                >
                  Assign target mode
                </Button>
              </div>
              {view.assignment?.effectiveMonth === month && (
                <p>
                  This month&apos;s assignment is fixed. Choose a later month for a different mode.
                </p>
              )}
              <FormDisclosure appearance="standard" className={styles.verification}>
                <summary>Verify provider cost</summary>
                <p>
                  Match an already verified AI meter to private provider evidence. The THB amount
                  must equal the reviewed original amount after FX conversion.
                </p>
                <form
                  onSubmit={(event) => {
                    event.preventDefault();
                    const fields = new FormData(event.currentTarget);
                    void submit("verify-provider-cost", {
                      sourceKey: String(fields.get("sourceKey")),
                      provider: String(fields.get("provider")),
                      statementId: String(fields.get("statementId")),
                      originalCurrency: String(fields.get("originalCurrency")).toUpperCase(),
                      originalMinor: Number(fields.get("originalMinor")),
                      fxNumerator: Number(fields.get("fxNumerator")),
                      fxDenominator: Number(fields.get("fxDenominator")),
                      providerCostSatang: Number(fields.get("providerCostSatang")),
                      evidenceId: String(fields.get("evidenceId")),
                    });
                  }}
                >
                  <Field appearance="standard">
                    AI source
                    <Select appearance="standard"
                      name="sourceKey"
                      required
                      value={selectedSourceKey}
                      onChange={(event) => {
                        setSelectedSourceKey(event.target.value);
                        setSelectedProvider(
                          pendingAiSources.find((line) => line.sourceKey === event.target.value)
                            ?.provider ?? "",
                        );
                      }}
                    >
                      {pendingAiSources.map((line) => (
                        <option key={line.sourceKey ?? ""} value={line.sourceKey ?? ""}>
                          {line.provider ?? "Provider unknown"} · {line.sourceKey} ·{" "}
                          {line.measuredQuantity}
                        </option>
                      ))}
                    </Select>
                  </Field>
                  <Field appearance="standard">
                    Provider
                    <TextInput appearance="standard"
                      name="provider"
                      required
                      maxLength={100}
                      value={selectedProvider}
                      onChange={(event) => setSelectedProvider(event.target.value)}
                    />
                  </Field>
                  <Field appearance="standard">
                    Statement ID
                    <TextInput appearance="standard" name="statementId" required maxLength={200} />
                  </Field>
                  <Field appearance="standard">
                    Original currency
                    <TextInput appearance="standard"
                      name="originalCurrency"
                      required
                      minLength={3}
                      maxLength={3}
                      placeholder="USD"
                    />
                  </Field>
                  <Field appearance="standard">
                    Original minor units
                    <TextInput appearance="standard" name="originalMinor" type="number" required min="0" step="1" />
                  </Field>
                  <Field appearance="standard">
                    FX numerator
                    <TextInput appearance="standard"
                      name="fxNumerator"
                      type="number"
                      required
                      min="1"
                      step="1"
                      defaultValue="1"
                    />
                  </Field>
                  <Field appearance="standard">
                    FX denominator
                    <TextInput appearance="standard"
                      name="fxDenominator"
                      type="number"
                      required
                      min="1"
                      step="1"
                      defaultValue="1"
                    />
                  </Field>
                  <Field appearance="standard">
                    Verified provider cost (satang)
                    <TextInput appearance="standard" name="providerCostSatang" type="number" required min="0" step="1" />
                  </Field>
                  <Field appearance="standard">
                    Available evidence ID
                    <TextInput appearance="standard"
                      key={uploadedEvidence?.id ?? "cost-evidence"}
                      name="evidenceId"
                      required
                      maxLength={100}
                      defaultValue={uploadedEvidence?.id ?? ""}
                    />
                  </Field>
                  <Button appearance="standard"
                    clientManaged variant="ghost"
                    type="submit"
                    disabled={
                      busy || !reasonValid || !pendingAiSources.length || view.state === "approved"
                    }
                  >
                    Verify cost
                  </Button>
                </form>
              </FormDisclosure>
              <FormDisclosure appearance="standard" className={styles.verification}>
                <summary>Reconcile provider statement</summary>
                <p>
                  The statement total must match the sum of matched provider source costs in
                  original minor units.
                </p>
                <form
                  onSubmit={(event) => {
                    event.preventDefault();
                    const fields = new FormData(event.currentTarget);
                    void submit("verify-statement", {
                      provider: String(fields.get("provider")),
                      statementId: String(fields.get("statementId")),
                      originalCurrency: String(fields.get("originalCurrency")).toUpperCase(),
                      providerTotalMinor: Number(fields.get("providerTotalMinor")),
                      evidenceId: String(fields.get("evidenceId")),
                    });
                  }}
                >
                  <Field appearance="standard">
                    Provider
                    <TextInput appearance="standard" name="provider" required maxLength={100} />
                  </Field>
                  <Field appearance="standard">
                    Statement ID
                    <TextInput appearance="standard" name="statementId" required maxLength={200} />
                  </Field>
                  <Field appearance="standard">
                    Original currency
                    <TextInput appearance="standard"
                      name="originalCurrency"
                      required
                      minLength={3}
                      maxLength={3}
                      placeholder="USD"
                    />
                  </Field>
                  <Field appearance="standard">
                    Provider statement total (minor units)
                    <TextInput appearance="standard" name="providerTotalMinor" type="number" required min="0" step="1" />
                  </Field>
                  <Field appearance="standard">
                    Available evidence ID
                    <TextInput appearance="standard"
                      key={uploadedEvidence?.id ?? "statement-evidence"}
                      name="evidenceId"
                      required
                      maxLength={100}
                      defaultValue={uploadedEvidence?.id ?? ""}
                    />
                  </Field>
                  <Button appearance="standard"
                    clientManaged variant="ghost"
                    type="submit"
                    disabled={busy || !reasonValid || view.state === "approved"}
                  >
                    Verify statement
                  </Button>
                </form>
              </FormDisclosure>
              <div className={styles.formRow}>
                <Button appearance="standard"
                  variant="ghost"
                  type="button"
                  disabled={busy || !reasonValid || !view.assignment || view.state === "approved"}
                  onClick={() => submit("rate-target", {})}
                >
                  Record rating review
                </Button>
                {view.assignment?.mode === "sandbox_invoice" && (
                  <Button appearance="standard"
                    variant="ghost"
                    type="button"
                    disabled={
                      busy ||
                      !reasonValid ||
                      !view.approval?.eligible ||
                      !view.price?.ratingEnabled ||
                      view.state === "approved"
                    }
                    onClick={() => submit("approve-target", {})}
                  >
                    Approve and issue Sandbox invoice
                  </Button>
                )}
              </div>
              {view.assignment?.mode === "shadow" && <p>Shadow rating produces no invoice.</p>}
            </div>
          )}
        </>
      )}
    </section>
  );
}
