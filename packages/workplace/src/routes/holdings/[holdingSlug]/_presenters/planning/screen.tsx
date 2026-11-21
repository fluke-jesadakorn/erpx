import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";
import { RecordLink as Link } from "@erpx/feature-records/components/record-link";
import { requireWorkContext, publicScope } from "@erpx/server-identity_access/public/authorization";
import { workspaceBase } from "@erpx/feature-work_projection/lib/workspace";
import { pageError } from "@erpx/server-work_projection/public/page-error";
import { productionWorkspace } from "@erpx/server-manufacturing/public/production";
import { ProductionConsole } from "@erpx/feature-manufacturing/components/production-console";
import { ownedPlanningWorkspace } from "@erpx/feature-manufacturing/public/native-planning-workspace-owned";
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
    ),
    native = await ownedPlanningWorkspace(a, holdingSlug).catch((e) =>
      pageError(e, holdingSlug),
    ),
    specificationName = (id: string): string =>
      String(
        data.specifications.find(
          (specification) => String(specification.id) === id,
        )?.name ?? id,
      ),
    plans = native
      ? native.scenarios.map((scenario) => ({
          ...scenario,
          specification_name: specificationName(scenario.specification_id),
          context_json: JSON.stringify(scenario.context),
        }))
      : data.plans,
    workspace = native
      ? {
          ...data,
          plans: plans as unknown as typeof data.plans,
          recommendations:
            native.recommendations as unknown as typeof data.recommendations,
          resources: native.resources as unknown as typeof data.resources,
          calendar: native.calendar as unknown as typeof data.calendar,
        }
      : data;
  return (
    <PageContent as="div" className="ui-page-content">
      <Link href={canonicalUiHref(`${base}/manufacturing`)}>Manufacturing</Link>
      <h1>Planning</h1>
      <p>
        Calculate traceable shortages, capacity and feasible dates, then
        independently release recommendations.
      </p>
      <ProductionConsole base={base} module="planning" data={workspace} reviewActor={{userId:a.userId,companyId:a.companyId,assignmentId:a.assignmentId,branchId:a.branchId,departmentId:a.departmentId}} />
      {native && (
        <Card as="section" appearance="standard" className="ui-panel">
          <h2>Native planning register</h2>
          <p>
            Current demand, shortage, capacity and recommendation facts read
            from the native Planning owner. A configured native failure is
            surfaced instead of silently restoring the retained list.
          </p>
          {native.scenarios.length ? (
            native.scenarios.map((scenario) => {
              const recommendation = native.recommendations.find(
                (row) => row.scenario_id === scenario.id,
              );
              const allocations = native.allocations.filter(
                (row) => row.scenario_id === scenario.id,
              );
              return (
                <details className="ui-panel" key={scenario.id}>
                  <summary>
                    {String(specificationName(scenario.specification_id))} ·{" "}
                    {scenario.status} · due {scenario.due_date} · version{" "}
                    {scenario.version}
                  </summary>
                  <p>
                    Demand {Number(scenario.demand_quantity_milli) / 1000} ·
                    capacity {Number(scenario.capacity_minutes)} minutes ·
                    feasible {recommendation?.feasible_date ?? "not calculated"}
                    {recommendation
                      ? ` · recommendation ${recommendation.status} v${recommendation.version}`
                      : ""}
                  </p>
                  <h3>Shortages</h3>
                  {scenario.shortages.length ? (
                    <ul>
                      {scenario.shortages.map((shortage) => (
                        <li key={shortage.productId}>
                          {shortage.productId} · required{" "}
                          {Number(shortage.requiredMilli) / 1000} · available{" "}
                          {Number(shortage.availableMilli) / 1000} · incoming{" "}
                          {Number(shortage.incomingMilli) / 1000} · shortage{" "}
                          {Number(shortage.shortageMilli) / 1000}
                        </li>
                      ))}
                    </ul>
                  ) : (
                    <p>No material shortage is recorded.</p>
                  )}
                  <h3>Committed capacity</h3>
                  {allocations.length ? (
                    <ul>
                      {allocations.map((allocation) => (
                        <li key={allocation.id}>
                          {allocation.resource_id} · {allocation.work_date} ·{" "}
                          {Number(allocation.minutes)} minutes
                        </li>
                      ))}
                    </ul>
                  ) : (
                    <p>No unreleased capacity allocation is recorded.</p>
                  )}
                </details>
              );
            })
          ) : (
            <p>No planning scenario is recorded in this scope.</p>
          )}
        </Card>
      )}
      {workspace.plans.map((scenario) => (
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
