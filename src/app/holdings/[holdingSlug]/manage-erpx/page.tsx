
import { PageContent } from "@/components/ui/patterns";
import { Table } from "@/components/ui/foundation";
import { TimeText } from "@/components/time/time-preference-context";
import { FeaturePage, FeatureWidget, WidgetGrid } from "@/components/features/feature-page";
import {
  LiveBanner,
  PageHeader,
  PreviewBanner,
  StatusBadge,
  Metric,
} from "@/components/ui/primitives";
import { workspaceChoices } from "@/server/workspace-choices";
import type { WorkspaceHoldingChoice } from "@/lib/workspace-choice";
import { notFound } from "next/navigation";
import { holdingResources, runtimeEnv } from "@/server/runtime";
import { activeHoldingBindings } from "@/server/runtime";
import { requireManageErpxContext } from "@/server/authorization";
import { summarizePlatformOperations } from "@/server/platform-operations";
import { PlatformJobRecovery } from "@/components/platform/platform-job-recovery";
import { PlatformMigrationRun } from "@/components/platform/platform-migration-run";
import { PlatformHoldingProvisioning } from "@/components/platform/platform-holding-provisioning";
import { platformProvisioningSnapshot } from "@/server/platform-operations";
import { resourceSlots } from "@/lib/holding-registry";
import { TableFrame, Card } from "@/components/ui/foundation";

const destinations: Record<string, [string, string]> = {
  home: [
    "Platform overview",
    "Control-plane operations are separated from every customer Holding.",
  ],
  work: [
    "Platform work",
    "Control-plane tasks appear here without granting access to customer data.",
  ],
  feed: ["Platform updates", "Operational announcements for Platform administrators."],
  inbox: ["Platform inbox", "Control-plane notifications remain isolated from Holding activity."],
  learn: ["Platform learning", "Guidance for safe control-plane administration."],
};

export default async function ManageErpxPage({
  params,
}: {
  params: Promise<{ holdingSlug: string }>;
}) {
  const { holdingSlug } = await params;
  if (holdingSlug !== "erpx") notFound();
  const destination = "home";
  const content = destinations[destination];
  if (!content) notFound();
  if (destination === "home") {
    const { platform: actor } = await requireManageErpxContext();
    const env = await runtimeEnv();
    const provisioning = await platformProvisioningSnapshot(env.CONTROL_DB, resourceSlots(env));
    const bindings = (await activeHoldingBindings()).filter((binding) => binding.id !== "h003");
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
      (choice): choice is WorkspaceHoldingChoice =>
        choice.kind === "holding" && choice.holdingSlug !== "erpx",
    );
    return (
      <PageContent as={FeaturePage} data-source="live">
        <PageHeader appearance="standard"
          eyebrow="ERPX"
          title="ERPX Management"
          description="Customer Holding health, provisioning, migrations and recovery."
        />
        <LiveBanner />
        <WidgetGrid>
          <FeatureWidget appearance="standard" id="platform-operations-health" title="Operational health">
            <div className="metric-grid">
              <Metric
                label="Customers"
                value={operations.totals.holdings}
                detail="Active customer bindings"
              />
              <Metric
                label="Pending jobs"
                value={operations.totals.pendingEvents}
                detail="Waiting for projection or worker retry"
              />
              <Metric
                label="Dead letters"
                value={operations.totals.deadLetterEvents}
                detail="Require operator recovery"
              />
              <Metric
                label="Claimed"
                value={operations.totals.claimedEvents}
                detail="Currently leased by a worker"
              />
            </div>
            <p className="muted">
              Snapshot <TimeText appearance="standard" value={operations.generatedAt} /> · platform role{" "}
              {operations.actorRole}
            </p>
            {operations.controlHealth.length ? (
              <ul>
                {operations.controlHealth.map((health) => (
                  <li key={health.service}>
                    <strong>{health.service}</strong> · heartbeat{" "}
                    <TimeText appearance="standard" value={health.heartbeatAt} />
                  </li>
                ))}
              </ul>
            ) : (
              <p className="muted">No control-plane heartbeat has been recorded yet.</p>
            )}
          </FeatureWidget>
          <FeatureWidget appearance="standard" id="platform-holding-operations" title="Holding operations">
            {operations.holdings.length ? (

                <TableFrame appearance="standard" label="Records"><Table>
                  <thead>
                    <tr>
                      <th>Holding</th>
                      <th>Status</th>
                      <th>Pending</th>
                      <th>Dead letters</th>
                      <th>Claimed</th>
                      <th>Jobs heartbeat</th>
                      <th>Oldest pending</th>
                    </tr>
                  </thead>
                  <tbody>
                    {operations.holdings.map((holding) => (
                      <tr key={holding.id}>
                        <td>
                          <strong>{holding.displayName}</strong>
                          <br />
                          <small>{holding.slug}</small>
                        </td>
                        <td>
                          <StatusBadge appearance="standard" status={holding.status} />
                          {holding.error && <p className="error">{holding.error}</p>}
                        </td>
                        <td>{holding.pendingEvents}</td>
                        <td>{holding.deadLetterEvents}</td>
                        <td>{holding.claimedEvents}</td>
                        <td>
                          {holding.jobHeartbeatAt ? (
                            <TimeText appearance="standard" value={holding.jobHeartbeatAt} />
                          ) : (
                            "—"
                          )}
                        </td>
                        <td>
                          {holding.oldestPendingAt ? (
                            <TimeText appearance="standard" value={holding.oldestPendingAt} />
                          ) : (
                            "—"
                          )}
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </Table></TableFrame>

            ) : (
              <p className="muted">No active Holding bindings are configured.</p>
            )}
            <p className="muted">
              This view reports persisted runtime state. Failed jobs below can be recovered only
              with a reason, an optimistic snapshot, an idempotency key, and a Platform audit entry.
            </p>
            {operations.holdings
              .filter((holding) => holding.deadLetters.length)
              .map((holding) => (
                <section key={holding.id}>
                  <h3>{holding.displayName} · failed jobs</h3>
                  <PlatformJobRecovery
                    holdingId={holding.id}
                    events={holding.deadLetters}
                    canRecover={actor.role === "platform_admin"}
                  />
                </section>
              ))}
            <h3>Schema migrations</h3>
            <p className="muted">
              Migration status is read from each Holding database. Running a migration uses the
              locked release runner and records an operator receipt in the control plane.
            </p>
            {operations.holdings.map((holding) => (
              <Card as="section" appearance="standard" key={`${holding.id}-migrations`} className="ui-panel">
                <h4>
                  {holding.displayName} <small>({holding.slug})</small>
                </h4>
                <PlatformMigrationRun
                  holdingId={holding.id}
                  migration={holding.migrations}
                  canRun={actor.role === "platform_admin"}
                />
              </Card>
            ))}
          </FeatureWidget>
          <FeatureWidget appearance="standard" id="platform-provisioning" title="Holding lifecycle">
            <PlatformHoldingProvisioning
              snapshot={provisioning}
              canProvision={actor.role === "platform_admin"}
            />
          </FeatureWidget>
          <FeatureWidget appearance="standard" id="platform-workspaces" title="Customer Holdings">
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
          </FeatureWidget>
        </WidgetGrid>
      </PageContent>
    );
  }
  return (
    <PageContent as={FeaturePage} data-source="sample">
      <PageHeader appearance="standard" eyebrow="Platform preview" title={content[0]} description={content[1]} />
      <WidgetGrid>
        <FeatureWidget appearance="standard" id="platform-preview" title={content[0]}>
          <PreviewBanner />
        </FeatureWidget>
      </WidgetGrid>
    </PageContent>
  );
}
