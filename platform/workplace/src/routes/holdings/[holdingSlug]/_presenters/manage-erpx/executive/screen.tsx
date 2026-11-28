import { nativePlatformAdminFailure, nativePlatformExecutiveForPresentation } from "@erpx/platform-administration-ui/lib/native-platform-administration";
import { headers } from "next/headers";
import { ApiError } from "@erpx/platform-runtime-server/public/http";
import {PlatformExecutiveSources} from "@erpx/records-ui/components/platform-executive-sources";
import {CollectionRecordLink} from "@erpx/records-ui/components/record-collection";
import { PageContent } from "@erpx/platform-ui/components/patterns";
import { Table } from "@erpx/platform-ui/components/foundation";
import { TimeText } from "@erpx/people-payroll-ui/components/time/time-preference-context";
import { FeaturePage, FeatureWidget, WidgetGrid } from "@erpx/platform-shared-ui/components/features/feature-page";
import { LiveBanner, Metric, PageHeader, StatusBadge } from "@erpx/platform-ui/components/primitives";
import { requireManageErpxContext } from "@erpx/identity-access-server/public/authorization";

import { TableFrame } from "@erpx/platform-ui/components/foundation";

export const dynamic = "force-dynamic";

export default async function PlatformExecutivePage() {
  const { work, platform } = await requireManageErpxContext();
  const snapshot = await nativePlatformExecutiveForPresentation(work, platform, await headers()).catch((error) => {
    const failure = nativePlatformAdminFailure(error);
    if (failure) throw new ApiError(failure.status, failure.code, failure.message);
    throw error;
  });
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
