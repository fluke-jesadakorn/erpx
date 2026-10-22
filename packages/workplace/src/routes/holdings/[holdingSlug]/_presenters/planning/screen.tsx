import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";
import { RecordLink as Link } from "@erpx/feature-records/components/record-link";
import { requireWorkContext, publicScope } from "@erpx/server-identity_access/public/authorization";
import { workspaceBase } from "@erpx/feature-work_projection/lib/workspace";
import { pageError } from "@erpx/server-work_projection/public/page-error";
import { productionWorkspace } from "@erpx/server-manufacturing/public/production";
import { ProductionConsole } from "@erpx/feature-manufacturing/components/production-console";
import { RecordDocumentDrawer } from "@erpx/feature-files/components/record-document-drawer";
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
    data = await productionWorkspace(a, "planning").catch((e) =>
      pageError(e, holdingSlug),
    );
  return (
    <PageContent as="div" className="ui-page-content">
      <Link href={canonicalUiHref(`${base}/manufacturing`)}>Manufacturing</Link>
      <h1>Planning</h1>
      <p>
        Calculate traceable shortages, capacity and feasible dates, then
        independently release recommendations.
      </p>
      <ProductionConsole base={base} module="planning" data={data} reviewActor={{userId:a.userId,companyId:a.companyId,assignmentId:a.assignmentId,branchId:a.branchId,departmentId:a.departmentId}} />
      {data.plans.map((scenario) => (
        <AttachmentFeatureSurface key={`files-${String(scenario.id)}`}>
          <Card as="section" appearance="standard" className="ui-panel">
            <div className="ui-section-header">
              <div>
                <h2><Link href={canonicalUiHref(`${base}/record-details/planning_scenario/${scenario.id}`)} record={{kind:"planning_scenario",id:String(scenario.id)}}>{String(scenario.specification_name)}</Link></h2>
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
