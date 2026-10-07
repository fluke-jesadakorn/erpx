import { RecordLink as Link } from "@/components/records/record-link";
import { requireWorkContext, publicScope } from "@/server/modules/identity_access/public/authorization";
import { workspaceBase } from "@/lib/workspace";
import { pageError } from "@/server/modules/work_projection/public/page-error";
import { productionDetail } from "@/server/modules/manufacturing/public/production";
import { ProductionConsole } from "@/components/modules/production-console";
import { RecordDocumentPanel } from "@/components/attachments/record-document-panel";
import { hasPermission } from "@/lib/permissions";
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
    data = await productionDetail(a, id).catch((e) =>
      pageError(e, holdingSlug),
    ),
    o = data.order;
  return (
    <PageContent as="div" className="ui-page-content">
      <Link href={`${base}/manufacturing`}>Manufacturing</Link>
      <h1>{String(o.specification_name)}</h1>
      <p>
        {String(o.mode)} · {String(o.status)} · version {String(o.version)}
      </p>
      <ProductionConsole base={base} module="manufacturing" detail={data} />
      {hasPermission(a,"records.read") && <RecordDocumentPanel appearance="standard"
        base={base}
        targetType="production_run"
        targetId={String(o.id)}
        expectedTargetVersion={Number(o.version)}
        editable={hasPermission(a,"records.write")}
        title="Production run files"
        purposes={[
          {key:"run_sheet",label:"Batch or run sheet",description:"Attach the controlled batch, machine, or production run record."},
          {key:"output_photo",label:"Output photo",description:"Attach output, packaging, label, or machine-counter photos."},
          {key:"scrap_photo",label:"Scrap photo",description:"Attach observed scrap or waste evidence."},
          {key:"qc_report",label:"QC report",description:"Attach the inspection report, certificate, or QC evidence."},
          {key:"rework_evidence",label:"Rework evidence",description:"Attach the approved rework instruction and completion evidence."},
        ]}
      />}
      <Card as="section" appearance="standard" className="ui-panel">
        <h2>Material and history</h2>
        {data.materials.map((x) => (
          <p key={String(x.id)}>
            {String(x.product_name)} · {Number(x.quantity_milli) / 1000}
          </p>
        ))}
        {data.history.map((x) => (
          <p key={String(x.id)}>
            v{String(x.version)} · {String(x.action)} · {String(x.reason)}
          </p>
        ))}
      </Card>
      {data.cost && (
        <Card as="section" appearance="standard" className="ui-panel">
          <h2>Actual cost and genealogy</h2>
          <p>
            Total minor units {String(data.cost.total_minor)} · output{" "}
            {Number(data.cost.output_quantity_milli) / 1000}
          </p>
          <p>{data.genealogy.length} material genealogy links</p>
          {data.lotGenealogy.map(link => <p key={String(link.id)}>{String(link.input_lot_code ?? link.input_serial_number)} → {String(link.output_lot_code ?? link.output_serial_number)} · {Number(link.input_quantity_milli) / 1000} input units</p>)}
        </Card>
      )}
      {data.postings.length > 0 && <Card as="section" appearance="standard" className="ui-panel"><h2>Production accounting</h2>{data.postings.map(posting => <p key={String(posting.id)}>
        {String(posting.phase)} · THB {(Number(posting.amount_minor) / 100).toFixed(2)} · {String(posting.journal_status)}
        {hasPermission(a, "finance.read") && <> · <Link href={`${base}/finance/journals/${String(posting.journal_id)}`}>Journal</Link></>}
      </p>)}</Card>}
    </PageContent>
  );
}
