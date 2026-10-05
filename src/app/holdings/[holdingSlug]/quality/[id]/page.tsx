import { TimeText } from "@/components/time/time-preference-context";
import { RecordLink as Link } from "@/components/records/record-link";
import { notFound } from "next/navigation";
import { QualityConsole } from "@/components/modules/quality-console";
import { StatusBadge } from "@/components/ui/primitives";
import { OperationSummary } from "@/components/workplace/operation-summary";
import { workspaceBase } from "@/lib/workspace";
import { requireWorkContext, publicScope } from "@/server/authorization";
import { ApiError } from "@/server/http";
import { qualityAlerts } from "@/server/quality-alerts";
import { qualityDetail } from "@/server/quality";
import { accessPage } from "@/server/page-error";
import styles from "./quality-detail.module.css";

export const dynamic = "force-dynamic";

function formatDate(value: string) {
  return new Intl.DateTimeFormat("en-GB", {
    day: "numeric",
    month: "short",
    year: "numeric",
    timeZone: "Asia/Bangkok",
  }).format(new Date(`${value}T00:00:00Z`));
}


export default async function Page({
  params,
  searchParams,
}: {
  params: Promise<{ holdingSlug: string; id: string }>;
  searchParams: Promise<{ return?: string }>;
}) {
  const { holdingSlug, id } = await params;
  const actor = await requireWorkContext(holdingSlug);
  const base = workspaceBase(publicScope(actor));
  let data: Awaited<ReturnType<typeof qualityDetail>>;
  let alerts: Awaited<ReturnType<typeof qualityAlerts>>;

  try {
    data = await qualityDetail(actor, id);
    alerts = (await qualityAlerts(actor)).filter((alert) => alert.hold_id === id);
  } catch (error) {
    if (error instanceof ApiError && error.status === 403) {
      return accessPage("permission", actor.holdingSlug);
    }
    if (error instanceof ApiError && error.status === 404) notFound();
    throw error;
  }

  const query = await searchParams;
  const returnQuery = new URLSearchParams(query.return ?? "");
  for (const key of [...returnQuery.keys()]) {
    if (!["view", "q", "queue", "type", "responsibility"].includes(key)) {
      returnQuery.delete(key);
    }
  }
  const returnToWork = returnQuery.size > 0;
  const returnHref = returnToWork
    ? `${base}/work?${returnQuery}`
    : `${base}/quality`;
  const returnLabel = returnToWork ? "← Back to work" : "← Quality holds";

  const hold = data.hold;
  const status = String(hold.status);
  const complete = ["released", "cancelled", "disposed"].includes(status);
  const blockedReason =
    !complete && status !== "disposition_pending" && !Number(hold.eligible_count)
      ? "No eligible independent recipient. The Company administrator must assign quality authority."
      : null;
  const nextAction = data.canDisposition
    ? "Request controlled scrap disposal"
    : data.canRelease
      ? "Approve release of the passing inspection"
      : data.canReviewCancellation
        ? "Review cancellation of the incorrect hold"
        : data.canInspect
          ? "Record an independent inspection"
          : data.canCancel
            ? "Request cancellation of the incorrect hold"
            : complete
              ? "Review the recorded outcome"
              : status === "disposition_pending"
                ? "Wait for Company Finance to review the controlled scrap"
                : "Wait for an eligible independent Quality reviewer";
  const responsible = complete
    ? "Completed"
    : status === "disposition_pending"
      ? "Company Finance"
      : status === "held" && hold.last_result === "fail"
        ? "Independent Quality manager"
        : status === "held"
          ? "Quality inspector"
          : "Independent Quality manager";
  const quantity = (Number(hold.held_quantity_milli ?? 0) / 1000).toLocaleString(
    "en-GB",
    { maximumFractionDigits: 3 },
  );
  const unit = String(hold.product_unit ?? "");
  const scope = hold.serial_number
    ? `Serial ${String(hold.serial_number)}`
    : hold.lot_code
      ? `Lot ${String(hold.lot_code)}`
      : "Entire product";

  return (
    <div className={`content-stack ${styles.page}`}>
      <Link className={styles.backLink} href={returnHref}>
        {returnLabel}
      </Link>

      <header className={styles.pageHeader}>
        <div>
          <p className={styles.eyebrow}>Quality hold</p>
          <h1>{String(hold.product_name)}</h1>
          <p className={styles.headerMeta}>
            {String(hold.warehouse_name)} <span aria-hidden="true">·</span> Due{" "}
            {formatDate(String(hold.due_date))}
          </p>
        </div>
        <StatusBadge appearance="standard" status={status} />
      </header>

      <div className={styles.recordLayout}>
        <div className={styles.contextColumn}>
          <section className={`card ${styles.holdCard}`} aria-labelledby="hold-details-heading">
            <div className={styles.sectionHeading}>
              <h2 id="hold-details-heading">Hold details</h2>
              <span>Version {String(hold.version)}</span>
            </div>

            <dl className={styles.facts}>
              <div>
                <dt>Warehouse</dt>
                <dd>{String(hold.warehouse_name)}</dd>
              </div>
              <div>
                <dt>Held quantity</dt>
                <dd>{quantity}{unit ? ` ${unit}` : ""}</dd>
              </div>
              <div>
                <dt>Hold scope</dt>
                <dd>{scope}</dd>
              </div>
              <div>
                <dt>Review due</dt>
                <dd>{formatDate(String(hold.due_date))}</dd>
              </div>
              {hold.expiry_date && (
                <div>
                  <dt>Stock expiry</dt>
                  <dd>{formatDate(String(hold.expiry_date))}</dd>
                </div>
              )}
            </dl>

            <div className={styles.reason}>
              <span>Reason for hold</span>
              <p>{String(hold.reason)}</p>
            </div>

            <div className={styles.recordMeta}>
              <span>Created by {String(hold.created_by)}</span>
              {hold.spec_id && (
                <span>
                  Specification {String(hold.spec_id)} · version {String(hold.spec_version)}
                </span>
              )}
            </div>

            {status === "cancelled" && (
              <p className={styles.outcome}>
                Cancelled by {String(hold.cancelled_by)} after independent review. This is
                not a passing inspection.
              </p>
            )}
            {status === "released" && (
              <p className={styles.outcome}>
                Released by {String(hold.released_by)}: {String(hold.release_reason)}
              </p>
            )}
            {status === "disposed" && (
              <p className={styles.outcome}>
                Disposed by {String(hold.disposed_by)} through the linked inventory and
                accounting adjustment.
              </p>
            )}
          </section>

          <section
            className={`card ${styles.inspectionSection}`}
            aria-labelledby="inspection-history-heading"
          >
            <div className={styles.sectionHeading}>
              <h2 id="inspection-history-heading">Inspection history</h2>
              <span>
                {data.inspections.length} {data.inspections.length === 1 ? "record" : "records"}
              </span>
            </div>

            {data.inspections.length ? (
              <div className={styles.inspections}>
                {data.inspections.map((inspection) => {
                  const createdAt = Number(inspection.created_at);
                  const measurements = data.measurements.filter(
                    (measurement) => measurement.inspection_id === inspection.id,
                  );
                  const attachments = data.attachments.filter(
                    (attachment) => attachment.inspection_id === inspection.id,
                  );

                  return (
                    <article className={styles.inspection} key={String(inspection.id)}>
                      <header className={styles.inspectionHeader}>
                        <div>
                          <StatusBadge appearance="standard" status={String(inspection.result)} />
                          <span>Recorded by {String(inspection.inspected_by)}</span>
                        </div>
                        {Number.isFinite(createdAt) ? <TimeText appearance="standard" value={createdAt} /> : "Time unavailable"}
                      </header>

                      <p className={styles.evidenceText}>{String(inspection.evidence)}</p>

                      {measurements.length > 0 && (
                        <dl className={styles.measurements}>
                          {measurements.map((measurement) => (
                            <div key={String(measurement.id)}>
                              <dt>{String(measurement.label)}</dt>
                              <dd>
                                <strong>
                                  {String(measurement.measured_value)} {String(measurement.unit)}
                                </strong>
                                <span>
                                  Accepted {measurement.minimum_value ?? "–"} to{" "}
                                  {measurement.maximum_value ?? "–"} ·{" "}
                                  {Number(measurement.passed) ? "Pass" : "Fail"}
                                </span>
                              </dd>
                            </div>
                          ))}
                        </dl>
                      )}

                      {attachments.length > 0 && (
                        <ul className={styles.attachments}>
                          {attachments.map((attachment) => (
                            <li key={String(attachment.id)}>
                              <Link href={`/api/v1${base}/attachments/${attachment.id}/content`}>
                                Evidence file · {String(attachment.original_name)}
                              </Link>
                              <details>
                                <summary>File integrity</summary>
                                <code>{String(attachment.checksum_sha256)}</code>
                              </details>
                            </li>
                          ))}
                        </ul>
                      )}

                      <p className={styles.reviewedVersion}>
                        Reviewed hold version {String(inspection.reviewed_version)}
                      </p>
                    </article>
                  );
                })}
              </div>
            ) : (
              <p className={styles.emptyEvidence}>
                {status === "cancelled"
                  ? "No inspection was performed."
                  : "Awaiting independent inspection."}
              </p>
            )}
          </section>
        </div>

        <aside className={styles.workflowColumn} aria-label="Quality workflow">
          <OperationSummary
            operation={{
              status,
              responsible,
              currentStep: complete ? null : status.replaceAll("_", " "),
              nextAction,
              blockedReason,
              dueAt: null,
              recordHref: null,
              actionKind: "work",
            }}
          >
            <p className="muted">Review due {formatDate(String(hold.due_date))}</p>
          </OperationSummary>
          <QualityConsole base={base} detail={data} alerts={alerts} />
        </aside>
      </div>
    </div>
  );
}
