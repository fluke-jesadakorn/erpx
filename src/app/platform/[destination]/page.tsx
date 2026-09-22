import { FeaturePage, FeatureTabs,FeatureWidget,WidgetGrid } from "@/components/features/feature-page";
import { LiveBanner, PageHeader,PreviewBanner,StatusBadge,Metric } from "@/components/ui/primitives";
import { workspaceChoices } from "@/server/workspace-choices";
import { notFound } from "next/navigation";
import { holdingResources, runtimeEnv } from "@/server/runtime";
import { activeHoldingBindings } from "@/server/runtime";
import { requirePlatformContext } from "@/server/authorization";
import { summarizePlatformOperations } from "@/server/platform-operations";
import { PlatformJobRecovery } from "@/components/platform/platform-job-recovery";
import { PlatformMigrationRun } from "@/components/platform/platform-migration-run";
import { PlatformHoldingProvisioning } from "@/components/platform/platform-holding-provisioning";
import { platformProvisioningSnapshot } from "@/server/platform-operations";
import { resourceSlots } from "@/lib/holding-registry";

const destinations: Record<string, [string, string]> = {
  home: ["Platform overview", "Control-plane operations are separated from every customer Holding."],
  work: ["Platform work", "Control-plane tasks appear here without granting access to customer data."],
  feed: ["Platform updates", "Operational announcements for Platform administrators."],
  inbox: ["Platform inbox", "Control-plane notifications remain isolated from Holding activity."],
  learn: ["Platform learning", "Guidance for safe control-plane administration."],
};

export default async function PlatformDestinationPage({ params }: PageProps<"/platform/[destination]">) {
  const { destination } = await params;
  const content = destinations[destination];
  if (!content) notFound();
  if (destination === "home") {
    const actor = await requirePlatformContext();
    const env = await runtimeEnv();
    const provisioning = await platformProvisioningSnapshot(env.CONTROL_DB, resourceSlots(env));
    const bindings = await activeHoldingBindings();
    const operations = await summarizePlatformOperations(
      actor,
      env.CONTROL_DB,
      await Promise.all(
        bindings.map(async (binding) => ({
          id: binding.id,
          slug: binding.slug,
          displayName: binding.displayName,
          db: (await holdingResources(binding.id)).write,
        })),
      ),
    );
    const choices = (await workspaceChoices()).filter(
      (choice) => choice.kind === "holding",
    );
    return (
      <FeaturePage className="content" data-source="live">
        <PageHeader
          eyebrow="Platform"
          title="Platform administration"
          description="Live control-plane visibility for every configured Holding. Customer records remain isolated."
        /><LiveBanner /><FeatureTabs/><WidgetGrid>
        <FeatureWidget id="platform-operations-health" title="Operational health">
          <div className="metric-grid">
            <Metric label="Holdings" value={operations.totals.holdings} detail="Active bindings" />
            <Metric label="Pending jobs" value={operations.totals.pendingEvents} detail="Waiting for projection or worker retry" />
            <Metric label="Dead letters" value={operations.totals.deadLetterEvents} detail="Require operator recovery" />
            <Metric label="Claimed" value={operations.totals.claimedEvents} detail="Currently leased by a worker" />
          </div>
          <p className="muted">Snapshot {new Date(operations.generatedAt).toISOString().replace("T", " ").slice(0, 19)} UTC · platform role {operations.actorRole}</p>
          {operations.controlHealth.length ? <ul>{operations.controlHealth.map((health) => <li key={health.service}><strong>{health.service}</strong> · heartbeat {new Date(health.heartbeatAt).toISOString().replace("T", " ").slice(0, 19)} UTC</li>)}</ul> : <p className="muted">No control-plane heartbeat has been recorded yet.</p>}
        </FeatureWidget>
        <FeatureWidget id="platform-holding-operations" title="Holding operations">
          {operations.holdings.length ? <div className="table-wrap"><table><thead><tr><th>Holding</th><th>Status</th><th>Pending</th><th>Dead letters</th><th>Claimed</th><th>Jobs heartbeat</th><th>Oldest pending</th></tr></thead><tbody>{operations.holdings.map((holding) => <tr key={holding.id}><td><strong>{holding.displayName}</strong><br /><small>{holding.slug}</small></td><td><StatusBadge status={holding.status} />{holding.error && <p className="error">{holding.error}</p>}</td><td>{holding.pendingEvents}</td><td>{holding.deadLetterEvents}</td><td>{holding.claimedEvents}</td><td>{holding.jobHeartbeatAt ? new Date(holding.jobHeartbeatAt).toISOString().replace("T", " ").slice(0, 19) + " UTC" : "—"}</td><td>{holding.oldestPendingAt ? new Date(holding.oldestPendingAt).toISOString().replace("T", " ").slice(0, 19) + " UTC" : "—"}</td></tr>)}</tbody></table></div> : <p className="muted">No active Holding bindings are configured.</p>}
          <p className="muted">This view reports persisted runtime state. Failed jobs below can be recovered only with a reason, an optimistic snapshot, an idempotency key, and a Platform audit entry.</p>
          {operations.holdings.filter((holding) => holding.deadLetters.length).map((holding) => <section key={holding.id}><h3>{holding.displayName} · failed jobs</h3><PlatformJobRecovery holdingId={holding.id} events={holding.deadLetters} canRecover={actor.role === "platform_admin"} /></section>)}
          <h3>Schema migrations</h3>
          <p className="muted">Migration status is read from each Holding database. Running a migration uses the locked release runner and records an operator receipt in the control plane.</p>
          {operations.holdings.map((holding) => <section key={`${holding.id}-migrations`} className="card">
            <h4>{holding.displayName} <small>({holding.slug})</small></h4>
            <PlatformMigrationRun holdingId={holding.id} migration={holding.migrations} canRun={actor.role === "platform_admin"} />
          </section>)}
        </FeatureWidget>
        <FeatureWidget id="platform-provisioning" title="Holding lifecycle">
          <PlatformHoldingProvisioning snapshot={provisioning} canProvision={actor.role === "platform_admin"} />
        </FeatureWidget>
        <FeatureWidget id="platform-workspaces" title="Business workspaces">
        {choices.length ? (
          <ul>
            {choices.map((choice) => (
              <li key={choice.holdingSlug}>
                <strong>{choice.label}</strong> ·{" "}
                {choice.companies.map((company) => company.label).join(", ")}
              </li>
            ))}
          </ul>
        ) : (
          <p>No business workspaces assigned.</p>
        )}
      </FeatureWidget></WidgetGrid></FeaturePage>
    );
  }
  return <FeaturePage className="content" data-source="sample">
    <PageHeader eyebrow="Platform preview" title={content[0]} description={content[1]} /><FeatureTabs/><WidgetGrid>
    <FeatureWidget id="platform-preview" title={content[0]}><PreviewBanner /></FeatureWidget>
  </WidgetGrid></FeaturePage>;
}
