import { notFound } from "next/navigation";
import { accessPage } from "@/server/page-error";
import { ApiError } from "@/server/http";
import Link from "next/link";
import { requireWorkContext, publicScope } from "@/server/authorization";
import { workspaceBase } from "@/lib/workspace";
import { qualityWorkspace } from "@/server/quality";
import { qualityAlerts } from "@/server/quality-alerts";
import { QualityConsole } from "@/components/modules/quality-console";
import { RecordDocumentDrawer } from "@/components/attachments/record-document-drawer";
import { AttachmentFeatureSurface } from "@/components/attachments/attachment-feature-context";
export const dynamic = "force-dynamic";
export default async function Page({
  params,
}: {
  params: Promise<{ holdingSlug: string }>;
}) {
  const actor = await requireWorkContext((await params).holdingSlug),
    base = workspaceBase(publicScope(actor));
  let data: Awaited<ReturnType<typeof qualityWorkspace>>,
    alerts: Awaited<ReturnType<typeof qualityAlerts>>;
  try {
    data = await qualityWorkspace(actor);
    alerts = await qualityAlerts(actor);
  } catch (error) {
    if (error instanceof ApiError && error.status === 403)
      return accessPage("permission", actor.holdingSlug);
    if (error instanceof ApiError && error.status === 404) notFound();
    throw error;
  }
  return (
    <div className="content-stack">
      <h1>Quality holds</h1>
      <p>
        A hold can quarantine a specific lot or serial, or the entire product at
        one warehouse. Existing reservations remain visible but cannot be
        delivered while their stock is held. Passing stock requires independent
        release; failed stock requires reviewed disposition.
      </p>
      <QualityConsole base={base} data={data} alerts={alerts} />
      {data.specifications.map((spec) => (
        <AttachmentFeatureSurface key={`files-${String(spec.id)}`}>
          <section className="card">
            <div className="section-head">
              <div>
                <h2>
                  {String(spec.product_name)} · {String(spec.name)}
                </h2>
                <p className="muted">
                  Controlled specification · version {String(spec.version)}
                </p>
              </div>
              <RecordDocumentDrawer
                base={base}
                targetType="inventory_quality_spec"
                targetId={String(spec.id)}
                expectedTargetVersion={Number(spec.version)}
                editable={data.canManageSpecs}
                title="Controlled quality specification"
                triggerLabel="Specification files"
                purposes={[
                  {
                    key: "controlled_specification",
                    label: "Controlled specification",
                    description:
                      "Attach the approved specification, test method, drawing, or certificate template for this exact revision.",
                    visibility:
                      "Visible to authorized Quality colleagues; a new file binds as a new immutable record revision.",
                  },
                ]}
              />
            </div>
          </section>
        </AttachmentFeatureSurface>
      ))}
      <section className="card">
        <h2>Review queue</h2>
        {data.holds.length ? (
          <table className="business-table">
            <thead>
              <tr>
                <th>Product / warehouse</th>
                <th>Lot or serial</th>
                <th>Quantity held</th>
                <th>Due date</th>
                <th>Status</th>
                <th>Next team</th>
              </tr>
            </thead>
            <tbody>
              {data.holds.map((h) => {
                const status = String(h.status),
                  terminal = ["released", "cancelled", "disposed"].includes(
                    status,
                  ),
                  next = terminal
                    ? "Completed"
                    : status === "disposition_pending"
                      ? "Company Finance"
                      : status === "held" && h.last_result === "fail"
                        ? "Independent Quality manager"
                        : Number(h.eligible_count)
                          ? status === "held"
                            ? "Quality inspector"
                            : "Independent Quality manager"
                          : "Blocked: administrator must assign an eligible recipient";
                return (
                  <tr key={String(h.id)}>
                    <td>
                      <Link href={`${base}/quality/${h.id}`}>
                        {String(h.product_name)} · {String(h.warehouse_name)}
                      </Link>
                    </td>
                    <td>
                      {h.serial_number
                        ? `Serial ${String(h.serial_number)}`
                        : h.lot_code
                          ? `Lot ${String(h.lot_code)}`
                          : "Entire product"}
                    </td>
                    <td>{Number(h.held_quantity_milli ?? 0) / 1000}</td>
                    <td>{String(h.due_date)}</td>
                    <td>{status}</td>
                    <td>{next}</td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        ) : (
          <p>No quality holds in your scope.</p>
        )}
      </section>
    </div>
  );
}
