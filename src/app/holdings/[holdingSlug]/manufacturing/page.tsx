import Link from "next/link";
import { requireWorkContext, publicScope } from "@/server/authorization";
import { workspaceBase } from "@/lib/workspace";
import { pageError } from "@/server/page-error";
import { productionWorkspace } from "@/server/production";
import { ProductionConsole } from "@/components/modules/production-console";
import { RecordDocumentPanel } from "@/components/attachments/record-document-panel";
import { AttachmentFeatureSurface } from "@/components/attachments/attachment-feature-context";
import { hasPermission } from "@/lib/permissions";
import { PageContent, Card } from "@/components/ui/foundation";
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
      <Link href={`${base}/planning`}>Planning</Link>
      <h1>Manufacturing</h1>
      <p>
        Control BOM and formula production from material reservation through
        output, QC, cost and genealogy.
      </p>
      <ProductionConsole base={base} module="manufacturing" data={data} />
      {data.specifications.map((specification) => (
        <AttachmentFeatureSurface key={`files-${String(specification.id)}`}><details className="ui-panel">
          <summary>
            Controlled files · {String(specification.name)} · version {String(specification.version)}
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
            href={`${base}/manufacturing/${x.id}`}
          >
            {String(x.specification_name)} · {String(x.status)}
          </Link>
        ))}
      </Card>
    </PageContent>
  );
}
