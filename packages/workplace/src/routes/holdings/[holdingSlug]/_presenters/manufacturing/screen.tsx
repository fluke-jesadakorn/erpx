import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";
import { RecordLink as Link } from "@erpx/feature-records/components/record-link";
import { requireWorkContext, publicScope } from "@erpx/server-identity_access/public/authorization";
import { workspaceBase } from "@erpx/feature-work_projection/lib/workspace";
import { pageError } from "@erpx/server-work_projection/public/page-error";
import { productionWorkspace } from "@erpx/server-manufacturing/public/production";
import { ProductionConsole } from "@erpx/feature-manufacturing/components/production-console";
import { RecordDocumentPanel } from "@erpx/feature-files/components/record-document-panel";
import { AttachmentFeatureSurface } from "@erpx/feature-files/components/attachment-feature-context";
import { hasPermission } from "@erpx/feature-identity_access/lib/permissions";
import { PageContent, Card } from "@erpx/ui/components/foundation";
export const dynamic = "force-dynamic";
export default async function Page({
  params,
}: {
  params: Promise<{ holdingSlug: string }>;
}) {
  const { holdingSlug } = await params,
    a = await requireWorkContext(holdingSlug),
    base = workspaceBase(publicScope(a)),
    data = await productionWorkspace(a, "manufacturing").catch((e) =>
      pageError(e, holdingSlug),
    );
  return (
    <PageContent as="div" className="ui-page-content">
      <Link href={canonicalUiHref(`${base}/planning`)}>Planning</Link>
      <h1>Manufacturing</h1>
      <p>
        Control BOM and formula production from material reservation through
        output, QC, cost and genealogy.
      </p>
      <ProductionConsole base={base} module="manufacturing" data={data} />
      {data.specifications.map((specification) => (
        <AttachmentFeatureSurface key={`files-${String(specification.id)}`}><details className="ui-panel">
          <summary>
            <Link href={canonicalUiHref(`${base}/record-details/manufacturing_spec/${specification.id}`)} record={{kind:"manufacturing_spec",id:String(specification.id)}}>{String(specification.name)} · View details</Link> · Controlled files · version {String(specification.version)}
          </summary>
          <RecordDocumentPanel appearance="standard"
            base={base}
            targetType="manufacturing_spec"
            targetId={String(specification.id)}
            expectedTargetVersion={Number(specification.version)}
            editable={hasPermission(a, "records.write")}
            title="BOM, formula, drawing and SOP files"
            purposes={[
              { key: "controlled_specification", label: "Controlled specification", description: "Attach the controlled manufacturing specification for this exact revision." },
              { key: "drawing", label: "Drawing", description: "Attach the approved drawing or production diagram for this revision." },
              { key: "sop", label: "SOP", description: "Attach the approved work instruction or standard operating procedure." },
              { key: "formula", label: "Formula", description: "Attach the restricted formula or recipe document for this revision.", visibility: "Restricted to authorized Manufacturing colleagues in this Company." },
            ]}
          />
        </details></AttachmentFeatureSurface>
      ))}
      <Card as="section" appearance="standard" className="ui-panel">
        <h2>Orders</h2>
        {data.orders.map((x) => (
          <Link
            className="ui-record-item"
            key={String(x.id)}
            href={canonicalUiHref(`${base}/manufacturing/${x.id}`)}
          >
            {String(x.specification_name)} · {String(x.status)}
          </Link>
        ))}
      </Card>
    </PageContent>
  );
}
