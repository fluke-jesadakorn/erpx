import Link from "next/link";
import { requireWorkContext, publicScope } from "@/server/authorization";
import { workspaceBase } from "@/lib/workspace";
import { pageError } from "@/server/page-error";
import { fieldServiceDetail } from "@/server/field-service";
import { FieldServiceConsole } from "@/components/modules/field-service-console";
import { RecordDocumentPanel } from "@/components/attachments/record-document-panel";
export const dynamic = "force-dynamic";
export default async function Page({
  params,
}: {
  params: Promise<{ holdingSlug: string; id: string }>;
}) {
  const { holdingSlug, id } = await params,
    a = await requireWorkContext(holdingSlug),
    base = workspaceBase(publicScope(a)),
    data = await fieldServiceDetail(a, id).catch((e) =>
      pageError(e, holdingSlug),
    ),
    x = data.order;
  return (
    <div className="content-stack">
      <Link href={`${base}/field-service`}>Field service</Link>
      <h1>{String(x.ticket_title)}</h1>
      <p>
        {String(x.customer_name)} · {String(x.site)} · {String(x.status)} ·
        technician {String(x.technician_name ?? x.technician_id)}
      </p>
      <FieldServiceConsole base={base} detail={data} />
      <RecordDocumentPanel
        base={base}
        targetType="field_service_visit"
        targetId={String(x.id)}
        expectedTargetVersion={Number(x.version)}
        editable={data.canWrite}
        title="Field visit files"
        purposes={[
          {key:"site_photo",label:"Site photo",description:"Attach arrival, site-condition, or completed-work photos."},
          {key:"technician_report",label:"Technician report",description:"Attach the technician's service report or checklist."},
          {key:"signed_job_sheet",label:"Signed job sheet",description:"Attach the customer-confirmed or signed job sheet."},
          {key:"exception_evidence",label:"Exception evidence",description:"Attach evidence supporting an access, safety, scope, or service exception."},
        ]}
      />
      <section className="card">
        <h2>Parts</h2>
        {data.parts.map((p) => (
          <p key={String(p.id)}>
            {String(p.sku)} · reserved {String(p.quantity_milli)} · issued{" "}
            {String(p.consumed_milli)} · released {String(p.released_milli)}
          </p>
        ))}
        {!data.parts.length && <p>No reserved parts.</p>}
      </section>
      <section className="card">
        <h2>Immutable visit history</h2>
        {data.history.map((e) => (
          <p key={String(e.id)}>
            Version {String(e.version)} · {String(e.action)} ·{" "}
            {String(e.reason)}
          </p>
        ))}
      </section>
      {data.billing && (
        <section className="card">
          <h2>Billing release</h2>
          <p>
            {String(data.billing.currency)} {String(data.billing.amount_minor)}{" "}
            minor units · {String(data.billing.status)}
          </p>
        </section>
      )}
    </div>
  );
}
