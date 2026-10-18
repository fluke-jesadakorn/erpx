import { platformExecutiveForPresentation } from "@/server/modules/platform_administration/public/presentation-queries";
import {PlatformExecutiveSources} from "@/components/records/platform-executive-sources";
import {CollectionRecordLink} from "@/components/records/record-collection";
import { PageContent } from "@/components/ui/patterns";
import { Table } from "@/components/ui/foundation";
import { TimeText } from "@/components/people_payroll/time/time-preference-context";
import { FeaturePage, FeatureWidget, WidgetGrid } from "@/components/shared/features/feature-page";
import { LiveBanner, Metric, PageHeader, StatusBadge } from "@/components/ui/primitives";
import { requireManageErpxContext } from "@/server/modules/identity_access/public/authorization";

import { requirePlatformExecutive } from "@/server/modules/platform_administration/public/platform-executive";
import { TableFrame } from "@/components/ui/foundation";

export const dynamic = "force-dynamic";

export default async function PlatformExecutivePage() {
  const actor = requirePlatformExecutive((await requireManageErpxContext()).platform);
  const snapshot = await platformExecutiveForPresentation(actor);
  return (
    <PageContent as={FeaturePage} data-source="live">
      <PageHeader appearance="standard"
        eyebrow="Control plane"
        title="Platform Executive"
        description="Provisioning and service health from the control plane. Customer Holding records are not included."
      />
      <LiveBanner />
      <PlatformExecutiveSources/>
      <WidgetGrid>
        {snapshot.metrics.map((metric) => (
          <FeatureWidget
            key={metric.id}
            id={`platform-executive-${metric.id}`}
            title={metric.label}
          >
            <div className="metric-grid">
              <Metric label={metric.label} value={metric.total} detail={metric.dateBasis} />
              <StatusBadge appearance="standard" status={metric.status} />
            </div>
            {metric.rows.length ? (
              <div>
                <TableFrame appearance="standard" label="Records"><Table>
                  <thead>
                    <tr>
                      <th>
                        {metric.id === "failed-provisioning" ? "UTC month" : "Service and status"}
                      </th>
                      <th>Count</th>
                    </tr>
                  </thead>
                  <tbody>
                    {metric.rows.map((row) => (
                      <tr key={`${row.label}:${row.value}`}>
                        <td><CollectionRecordLink kind="platform_executive_sources" id={row.label} args={{metric:metric.id,from:snapshot.filters.from,to:snapshot.filters.to}}>{row.label}</CollectionRecordLink></td>
                        <td>{row.value}</td>
                      </tr>
                    ))}
                  </tbody>
                </Table></TableFrame>
              </div>
            ) : (
              <p className="muted">No matching control-plane records.</p>
            )}
            <details>
              <summary>Source and formula</summary>
              <p>{metric.formula}</p>
              <p className="muted">
                Source revision {metric.sourceReferences[0]?.revision.slice(0, 12)} · Referenced{" "}
                <TimeText appearance="standard"
                  value={metric.sourceReferences[0]?.referencedAt ?? snapshot.referenceAt}
                />
              </p>
            </details>
          </FeatureWidget>
        ))}
      </WidgetGrid>
      <p className="muted">
        Reference time <TimeText appearance="standard" value={snapshot.referenceAt} /> · Provisioning period{" "}
        {snapshot.filters.from} to {snapshot.filters.to}
      </p>
    </PageContent>
  );
}
