import { notFound } from "next/navigation";
import Link from "next/link";
import { accessPage } from "@/server/page-error";
import { ApiError } from "@/server/http";
import { requireWorkContext, publicScope } from "@/server/authorization";
import { workspaceBase } from "@/lib/workspace";
import { qualityDetail } from "@/server/quality";
import { qualityAlerts } from "@/server/quality-alerts";
import { QualityConsole } from "@/components/modules/quality-console";
import { OperationSummary } from "@/components/workplace/operation-summary";

export const dynamic = "force-dynamic";

export default async function Page({ params }: { params: Promise<{ holdingSlug: string; id: string }> }) {
  const { holdingSlug, id } = await params;
  const actor = await requireWorkContext(holdingSlug);
  const base = workspaceBase(publicScope(actor));
  let data: Awaited<ReturnType<typeof qualityDetail>>;
  let alerts: Awaited<ReturnType<typeof qualityAlerts>>;
  try {
    data = await qualityDetail(actor, id);
    alerts = (await qualityAlerts(actor)).filter(alert => alert.hold_id === id);
  } catch (error) {
    if (error instanceof ApiError && error.status === 403) return accessPage("permission", actor.holdingSlug);
    if (error instanceof ApiError && error.status === 404) notFound();
    throw error;
  }
  const h = data.hold;
  const status = String(h.status);
  const complete = ["released", "cancelled", "disposed"].includes(status);
  const blockedReason = !complete && status !== "disposition_pending" && !Number(h.eligible_count)
    ? "No eligible independent recipient. The Company administrator must assign quality authority."
    : null;
  const nextAction = data.canDisposition ? "Request controlled scrap disposal"
    : data.canRelease ? "Approve release of the passing inspection"
    : data.canReviewCancellation ? "Review cancellation of the incorrect hold"
    : data.canInspect ? "Record an independent inspection"
    : data.canCancel ? "Request cancellation of the incorrect hold"
    : complete ? "Review the recorded outcome"
    : status === "disposition_pending" ? "Wait for Company Finance to review the controlled scrap"
    : "Wait for an eligible independent Quality reviewer";
  const responsible = complete ? "Completed" : status === "disposition_pending" ? "Company Finance"
    : status === "held" && h.last_result === "fail" ? "Independent Quality manager"
    : status === "held" ? "Quality inspector" : "Independent Quality manager";
  return <div className="content-stack">
    <Link href={`${base}/quality`}>← Quality holds</Link>
    <h1>{String(h.product_name)} · Quality hold</h1>
    <OperationSummary operation={{ status, responsible, currentStep: complete ? null : status.replaceAll("_", " "), nextAction, blockedReason, dueAt: null, recordHref: null, actionKind: "work" }}>
      <p className="muted">Due {String(h.due_date)}</p>
    </OperationSummary>
    <section className="card">
      <p>{String(h.warehouse_name)} · {status} · Due {String(h.due_date)}</p>
      <p><strong>{h.serial_number ? `Serial ${String(h.serial_number)}` : h.lot_code ? `Lot ${String(h.lot_code)}` : "Entire product"}</strong> · {Number(h.held_quantity_milli ?? 0) / 1000} held{h.expiry_date ? ` · expires ${String(h.expiry_date)}` : ""}</p>
      <p>{String(h.reason)}</p>
      <p>Created by {String(h.created_by)} · Version {String(h.version)}</p>
      {h.spec_id && <p>Measurement specification {String(h.spec_id)} · version {String(h.spec_version)} was captured when this hold was created.</p>}
      {status === "cancelled" && <p>Cancelled by {String(h.cancelled_by)} after independent review. This is not a passing inspection.</p>}
      {status === "released" && <p>Released by {String(h.released_by)}: {String(h.release_reason)}</p>}
      {status === "disposed" && <p>Disposed by {String(h.disposed_by)} through the linked inventory and accounting adjustment.</p>}
    </section>
    <section className="card"><h2>Inspection evidence</h2>
      {data.inspections.length ? data.inspections.map(inspection => <article key={String(inspection.id)}>
        <h3>{String(inspection.result)} · {String(inspection.inspected_by)}</h3>
        <p>{String(inspection.evidence)}</p>
        {data.measurements.filter(measurement => measurement.inspection_id === inspection.id).map(measurement => <p key={String(measurement.id)}>{String(measurement.label)}: <strong>{String(measurement.measured_value)} {String(measurement.unit)}</strong> · accepted {measurement.minimum_value ?? "–"} to {measurement.maximum_value ?? "–"} · {Number(measurement.passed) ? "pass" : "fail"}</p>)}
        {data.attachments.filter(attachment => attachment.inspection_id === inspection.id).map(attachment => <p key={String(attachment.id)}><Link href={`/api/v1${base}/attachments/${attachment.id}/content`}>Evidence file · {String(attachment.original_name)}</Link> · {String(attachment.checksum_sha256)}</p>)}
        <p>Reviewed version {String(inspection.reviewed_version)} · {new Date(Number(inspection.created_at)).toISOString()}</p>
      </article>) : <p>{status === "cancelled" ? "No inspection was performed." : "Awaiting independent inspection."}</p>}
    </section>
    <QualityConsole base={base} detail={data} alerts={alerts}/>
  </div>;
}
