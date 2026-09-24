import { FeaturePage, FeatureWidget, WidgetGrid } from "@/components/features/feature-page";
import { LiveBanner, Metric, PageHeader, StatusBadge } from "@/components/ui/primitives";
import { requireManageErpxContext } from "@/server/authorization";
import { runtimeEnv } from "@/server/runtime";
import { platformExecutiveSnapshot, requirePlatformExecutive } from "@/server/platform-executive";

export const dynamic = "force-dynamic";

export default async function PlatformExecutivePage() {
  const actor = requirePlatformExecutive((await requireManageErpxContext()).platform);
  const env = await runtimeEnv();
  const snapshot = await platformExecutiveSnapshot(actor, env.CONTROL_DB, env.CONTROL_READ_DB);
  return (
    <FeaturePage className="content" data-source="live">
      <PageHeader
        eyebrow="Control plane"
        title="Platform Executive"
        description="Provisioning and service health from the control plane. Customer Holding records are not included."
      />
      <LiveBanner />
      <WidgetGrid>
        {snapshot.metrics.map((metric) => (
          <FeatureWidget key={metric.id} id={`platform-executive-${metric.id}`} title={metric.label}>
            <div className="metric-grid">
              <Metric label={metric.label} value={metric.total} detail={metric.dateBasis} />
              <StatusBadge status={metric.status} />
            </div>
            {metric.rows.length ? (
              <div className="table-wrap">
                <table>
                  <thead><tr><th>{metric.id === "failed-provisioning" ? "UTC month" : "Service and status"}</th><th>Count</th></tr></thead>
                  <tbody>{metric.rows.map((row) => <tr key={`${row.label}:${row.value}`}><td>{row.label}</td><td>{row.value}</td></tr>)}</tbody>
                </table>
              </div>
            ) : <p className="muted">No matching control-plane records.</p>}
            <details>
              <summary>Source and formula</summary>
              <p>{metric.formula}</p>
              <p className="muted">Source revision {metric.sourceReferences[0]?.revision.slice(0, 12)} · Referenced {new Date(metric.sourceReferences[0]?.referencedAt ?? snapshot.referenceAt).toISOString()} UTC</p>
            </details>
          </FeatureWidget>
        ))}
      </WidgetGrid>
      <p className="muted">Reference time {new Date(snapshot.referenceAt).toISOString()} UTC · Provisioning period {snapshot.filters.from} to {snapshot.filters.to}</p>
    </FeaturePage>
  );
}
