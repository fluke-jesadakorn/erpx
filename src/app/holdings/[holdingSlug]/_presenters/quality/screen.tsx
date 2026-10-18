import {QualityEnvironment} from "@/components/quality/quality-purchase-context";
import {qualityPurchaseControls} from "@/server/modules/quality/public/purchases";
import { canonicalUiHref } from "@/lib/work_projection/destination-registry";
import { Table } from "@/components/ui/foundation";
import { notFound } from "next/navigation";
import { accessPage } from "@/server/modules/work_projection/public/page-error";
import { ApiError } from "@/server/modules/runtime/public/http";
import { RecordLink as Link } from "@/components/records/record-link";
import { requireWorkContext, publicScope } from "@/server/modules/identity_access/public/authorization";
import { workspaceBase } from "@/lib/work_projection/workspace";
import { qualityWorkspace } from "@/server/modules/quality/public/quality";
import { qualityAlerts } from "@/server/modules/quality/public/quality-alerts";
import { QualityConsole } from "@/components/quality/quality-console";
import { RecordDocumentDrawer } from "@/components/files/record-document-drawer";
import { AttachmentFeatureSurface } from "@/components/files/attachment-feature-context";
import { PageContent, Card, TableFrame } from "@/components/ui/foundation";
export const dynamic = "force-dynamic";
export default async function Page({ params }: { params: Promise<{ holdingSlug: string }> }) {
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
  const purchase=await qualityPurchaseControls(actor),observations=purchase?await Promise.all(data.holds.map(async row=>[String(row.id),await qualityPurchaseControls(actor,String(row.id))] as const)):[],sources=Object.fromEntries(observations.flatMap(([key,value])=>value?[[key,value]]:[]));
  for(const alert of alerts)if(sources[alert.hold_id])sources[alert.id]=sources[alert.hold_id];
  return (
    <QualityEnvironment purchase={purchase} sources={sources}><PageContent as="div" className="ui-page-content">
      <h1>Quality holds</h1>
      <p>
        A hold can quarantine a specific lot or serial, or the entire product at one warehouse.
        Existing reservations remain visible but cannot be delivered while their stock is held.
        Passing stock requires independent release; failed stock requires reviewed disposition.
      </p>
      <QualityConsole base={base} data={data} alerts={alerts} />
      {data.specifications.map((spec) => (
        <AttachmentFeatureSurface key={`files-${String(spec.id)}`}>
          <Card as="section" appearance="standard" className="ui-panel">
            <div className="ui-section-header">
              <div>
                <h2>
                  {String(spec.product_name)} · {String(spec.name)}
                </h2>
                <p className="muted">Controlled specification · version {String(spec.version)}</p>
              </div>
              <RecordDocumentDrawer
                base={base}
                targetType="inventory_quality_spec"
                targetId={String(spec.id)}
                expectedTargetVersion={Number(spec.version)}
                editable={data.canManageSpecs&&purchase?.newWork!==false}
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
          </Card>
        </AttachmentFeatureSurface>
      ))}
      <Card as="section" appearance="standard" className="ui-panel">
        <h2>Review queue</h2>
        {data.holds.length ? (
          <TableFrame appearance="standard" label="Records"><Table className="business-table">
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
                  terminal = ["released", "cancelled", "disposed"].includes(status),
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
                      <Link href={canonicalUiHref(`${base}/quality/${h.id}`)}>
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
          </Table></TableFrame>
        ) : (
          <p>No quality holds in your scope.</p>
        )}
      </Card>
    </PageContent></QualityEnvironment>
  );
}
