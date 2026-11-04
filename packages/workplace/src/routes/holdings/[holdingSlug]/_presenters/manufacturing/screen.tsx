import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";
import { RecordLink as Link } from "@erpx/feature-records/components/record-link";
import { requireWorkContext, publicScope } from "@erpx/server-identity_access/public/authorization";
import { workspaceBase } from "@erpx/feature-work_projection/lib/workspace";
import { pageError } from "@erpx/server-work_projection/public/page-error";
import { productionWorkspace } from "@erpx/server-manufacturing/public/production";
import { ProductionConsole } from "@erpx/feature-manufacturing/components/production-console";
import { ownedManufacturingBom } from "@erpx/feature-manufacturing/public/native-manufacturing-bom-owned";
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
    ),
    nativeBom = await ownedManufacturingBom(a, holdingSlug).catch((e) =>
      pageError(e, holdingSlug),
    ),
    recordPurchases =
      "recordPurchases" in data ? data.recordPurchases : undefined,
    nativeSpecifications = nativeBom?.specifications.filter(
      (specification) =>
        !recordPurchases ||
        recordPurchases[String(specification.id)]?.history === true,
    ),
    workspace =
      nativeBom && nativeSpecifications
        ? {
            ...data,
            specifications:
              nativeSpecifications as unknown as typeof data.specifications,
          }
        : data;
  const productName = (id: string) => {
    const product = data.products.find((row) => String(row.id) === id);
    return product ? String(product.name) : id;
  };
  return (
    <PageContent as="div" className="ui-page-content">
      <Link href={canonicalUiHref(`${base}/planning`)}>Planning</Link>
      <h1>Manufacturing</h1>
      <p>
        Control BOM and formula production from material reservation through
        output, QC, cost and genealogy.
      </p>
      <ProductionConsole base={base} module="manufacturing" data={workspace} />
      {nativeBom && (
        <Card as="section" appearance="standard" className="ui-panel">
          <h2>BOM and routing register</h2>
          <p>
            Current specifications, component quantities and ordered routing
            operations read from the native Manufacturing owner.
          </p>
          {nativeSpecifications?.map((specification) => (
            <details className="ui-panel" key={String(specification.id)}>
              <summary>
                <Link
                  href={canonicalUiHref(
                    `${base}/record-details/manufacturing_spec/${specification.id}`,
                  )}
                  record={{
                    kind: "manufacturing_spec",
                    id: String(specification.id),
                  }}
                >
                  {String(specification.name)}
                </Link>{" "}
                · {String(specification.status)} · {String(specification.mode)}{" "}
                · version {String(specification.version)}
              </summary>
              <p>
                Batch {Number(specification.batch_quantity_milli) / 1000} ·
                Expected yield {Number(specification.expected_yield_bps) / 100}%
                · Output product {productName(String(specification.output_product_id))}
              </p>
              <h3>Components</h3>
              <ul>
                {specification.lines.map((line) => (
                  <li key={String(line.id)}>
                    {productName(String(line.product_id))} ·{" "}
                    {Number(line.quantity_milli) / 1000} per batch
                    {line.substitution_group
                      ? ` · Substitute group ${String(line.substitution_group)}`
                      : ""}
                  </li>
                ))}
              </ul>
              <h3>Routing</h3>
              {specification.operations.length ? (
                <ol>
                  {specification.operations.map((operation) => (
                    <li key={String(operation.id)}>
                      {String(operation.name)} · {String(operation.resource_name)}{" "}
                      · setup {String(operation.setup_minutes)} min · run{" "}
                      {String(operation.run_minutes_per_batch)} min per batch
                    </li>
                  ))}
                </ol>
              ) : (
                <p>No routing operations are defined.</p>
              )}
            </details>
          ))}
        </Card>
      )}
      {workspace.specifications.map((specification) => (
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
