import Link from "next/link";
import { requireWorkContext, publicScope } from "@/server/authorization";
import { workspaceBase } from "@/lib/workspace";
import { pageError } from "@/server/page-error";
import { productionDetail } from "@/server/production";
import { ProductionConsole } from "@/components/modules/production-console";
import { RecordDocumentPanel } from "@/components/attachments/record-document-panel";
import { hasPermission } from "@/lib/permissions";
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
    <div className="content-stack">
      <Link href={`${base}/manufacturing`}>Manufacturing</Link>
      <h1>{String(o.specification_name)}</h1>
      <p>
        {String(o.mode)} · {String(o.status)} · version {String(o.version)}
      </p>
      <ProductionConsole base={base} module="manufacturing" detail={data} />
      {hasPermission(a,"records.read") && <RecordDocumentPanel
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
      <section className="card">
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
      </section>
      {data.cost && (
        <section className="card">
          <h2>Actual cost and genealogy</h2>
          <p>
            Total minor units {String(data.cost.total_minor)} · output{" "}
            {Number(data.cost.output_quantity_milli) / 1000}
          </p>
          <p>{data.genealogy.length} material genealogy links</p>
        </section>
      )}
    </div>
  );
}
