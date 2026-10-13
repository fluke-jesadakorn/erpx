import { RecordLink as Link } from "@/components/records/record-link";
import { requireWorkContext, publicScope } from "@/server/modules/identity_access/public/authorization";
import { workspaceBase } from "@/lib/work_projection/workspace";
import { pageError } from "@/server/modules/work_projection/public/page-error";
import { fieldServiceDetail } from "@/server/modules/field_service/public/field-service";
import { FieldServiceConsole } from "@/components/records/loaders/modules--field-service-console";
import { RecordDocumentPanel } from "@/components/records/loaders/attachments--record-document-panel";
import { minorToDecimal } from "@/lib/finance/finance-money";
import { hasPermission } from "@/lib/identity_access/permissions";
import { PageContent, Card } from "@/components/ui/foundation";
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
    <PageContent as="div" className="ui-page-content">
      <Link href={`${base}/field-service`}>Field service</Link>
      <h1>{String(x.ticket_title)}</h1>
      <p>
        {String(x.customer_name)} · {String(x.site)} · {String(x.status)} ·
        technician {String(x.technician_name ?? x.technician_id)}
      </p>
      <FieldServiceConsole base={base} detail={data} />
      <RecordDocumentPanel appearance="standard"
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
      <Card as="section" appearance="standard" className="ui-panel">
        <h2>Parts</h2>
        {data.parts.map((p) => (
          <p key={String(p.id)}>
            {String(p.sku)} · reserved {minorToDecimal(Number(p.quantity_milli),3)} · issued{" "}
            {minorToDecimal(Number(p.consumed_milli),3)} · released {minorToDecimal(Number(p.released_milli),3)}
          </p>
        ))}
        {!data.parts.length && <p>No reserved parts.</p>}
      </Card>
      {data.consumptions.length>0 && <Card as="section" appearance="standard" className="ui-panel"><h2>Issued spare costs</h2>
        {data.consumptions.map(cost => <p key={String(cost.id)}>{String(cost.product_name)} · {minorToDecimal(Number(cost.quantity_milli),3)} · {String(cost.currency)} {minorToDecimal(Number(cost.value_minor))} · {String(cost.journal_status)}
          {hasPermission(a,"finance.read") && <> · <Link href={`${base}/finance/journals/${cost.journal_id}`}>Review accounting</Link></>}
        </p>)}
      </Card>}
      <Card as="section" appearance="standard" className="ui-panel">
        <h2>Immutable visit history</h2>
        {data.history.map((e) => (
          <p key={String(e.id)}>
            Version {String(e.version)} · {String(e.action)} ·{" "}
            {String(e.reason)}
          </p>
        ))}
      </Card>
      {data.billing && (
        <Card as="section" appearance="standard" className="ui-panel">
          <h2>Billing release</h2>
          <p>
            {String(data.billing.currency)} {minorToDecimal(Number(data.billing.amount_minor))}{" "}
            · {String(data.billing.status)}
          </p>
        </Card>
      )}
    </PageContent>
  );
}
