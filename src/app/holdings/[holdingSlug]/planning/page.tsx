import { RecordLink as Link } from "@/components/records/record-link";
import { requireWorkContext, publicScope } from "@/server/authorization";
import { workspaceBase } from "@/lib/workspace";
import { pageError } from "@/server/page-error";
import { productionWorkspace } from "@/server/production";
import { ProductionConsole } from "@/components/modules/production-console";
import { RecordDocumentDrawer } from "@/components/attachments/record-document-drawer";
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
    data = await productionWorkspace(a, "planning").catch((e) =>
      pageError(e, holdingSlug),
    );
  return (
    <PageContent as="div" className="ui-page-content">
      <Link href={`${base}/manufacturing`}>Manufacturing</Link>
      <h1>Planning</h1>
      <p>
        Calculate traceable shortages, capacity and feasible dates, then
        independently release recommendations.
      </p>
      <ProductionConsole base={base} module="planning" data={data} />
      {data.plans.map((scenario) => (
        <AttachmentFeatureSurface key={`files-${String(scenario.id)}`}>
          <Card as="section" appearance="standard" className="ui-panel">
            <div className="ui-section-header">
              <div>
                <h2><Link href={`${base}/record-details/planning_scenario/${scenario.id}`} record={{kind:"planning_scenario",id:String(scenario.id)}}>{String(scenario.specification_name)}</Link></h2>
                <p className="muted">
                  Planning sources · due {String(scenario.due_date)}
                </p>
              </div>
              <RecordDocumentDrawer
                base={base}
                targetType="planning_scenario"
                targetId={String(scenario.id)}
                expectedTargetVersion={Number(scenario.version)}
                editable={hasPermission(a, "records.write")}
                title="Scenario assumptions and capacity sources"
                purposes={[
                  {
                    key: "assumption_source",
                    label: "Assumption source",
                    description:
                      "Attach the demand, timing, or other reviewed assumption used by this planning scenario.",
                  },
                  {
                    key: "capacity_source",
                    label: "Capacity source",
                    description:
                      "Attach the roster, machine, supplier, or other reviewed capacity source for this scenario.",
                  },
                  {
                    key: "source_document",
                    label: "Other planning source",
                    description:
                      "Attach another source document used by this real planning workflow.",
                  },
                ]}
                triggerLabel="Sources"
              />
            </div>
          </Card>
        </AttachmentFeatureSurface>
      ))}
    </PageContent>
  );
}
