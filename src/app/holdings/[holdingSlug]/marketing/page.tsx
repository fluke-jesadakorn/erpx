import { ActionLink as UiActionLink } from "@/components/ui/foundation";
import { Table } from "@/components/ui/foundation";
import Link from "next/link";
import { requireWorkContext, publicScope } from "@/server/authorization";
import { workspaceBase } from "@/lib/workspace";
import { pageError } from "@/server/page-error";
import { marketingWorkspace } from "@/server/marketing";
import { MarketingConsole } from "@/components/modules/marketing-console";
import {
  marketingAnalytics,
  analyticsOptions,
  defaultAnalyticsRange,
} from "@/server/crm-marketing-report";
import { AnalyticsReport } from "@/components/modules/analytics-report";
import { RecordDocumentPanel } from "@/components/attachments/record-document-panel";
import { MediaPublicationPanel } from "@/components/attachments/media-publication-panel";
import { AttachmentFeatureSurface } from "@/components/attachments/attachment-feature-context";
import { hasPermission } from "@/lib/permissions";
import { PageContent, Card, TableFrame } from "@/components/ui/foundation";
export const dynamic = "force-dynamic";
export default async function Page({
  params,
  searchParams,
}: {
  params: Promise<{ holdingSlug: string }>;
  searchParams: Promise<{
    from?: string;
    to?: string;
    branchId?: string;
    propertyId?: string;
    campaignId?: string;
    assetId?: string;
    ownerId?: string;
  }>;
}) {
  const { holdingSlug } = await params,
    a = await requireWorkContext(holdingSlug),
    base = workspaceBase(publicScope(a)),
    data = await marketingWorkspace(a).catch((e) => pageError(e, holdingSlug)),
    q = await searchParams,
    defaults = defaultAnalyticsRange(),
    raw = {
      from: q.from ?? defaults.from,
      to: q.to ?? defaults.to,
      branchId: q.branchId,
      propertyId: q.propertyId,
      campaignId: q.campaignId,
      ownerId: q.ownerId,
    },
    [report, options] = await Promise.all([marketingAnalytics(a, raw), analyticsOptions(a)]).catch(
      (e) => pageError(e, holdingSlug),
    ),
    selectedCampaign = q.campaignId
      ? data.campaigns.find((c) => String(c.id) === q.campaignId)
      : null,
    selectedAsset = q.assetId
      ? data.assets.find((asset) => asset.kind === "content" && String(asset.id) === q.assetId)
      : null;
  return (
    <PageContent as="div" className="ui-page-content">
      <Link href={`${base}/crm`}>CRM</Link>
      <h1>Marketing</h1>
      <p>
        Review forms, content, domains and provider readiness before capturing consented, attributed
        leads for CRM.
      </p>
      <AnalyticsReport kind="marketing" base={base} report={report} options={options} />
      <Card as="section" appearance="standard" className="ui-panel">
        <h2>Lead intake by campaign</h2>
        <p>Candidate counts by recorded campaign and source. Contact details are not included.</p>
        {data.campaignSummary.length ? (
          <TableFrame appearance="standard" label="Records"><Table className="business-table">
            <thead>
              <tr>
                <th>Campaign</th>
                <th>Source</th>
                <th>Captured</th>
                <th>Pending</th>
                <th>CRM handoffs</th>
                <th>Rejected</th>
              </tr>
            </thead>
            <tbody>
              {data.campaignSummary.map((row) => (
                <tr key={`${String(row.campaign)}:${String(row.source)}`}>
                  <td>{String(row.campaign)}</td>
                  <td>{String(row.source)}</td>
                  <td>{Number(row.captured)}</td>
                  <td>{Number(row.pending)}</td>
                  <td>{Number(row.handed_off)}</td>
                  <td>{Number(row.rejected)}</td>
                </tr>
              ))}
            </tbody>
          </Table></TableFrame>
        ) : (
          <p>No lead candidates have been captured yet.</p>
        )}
      </Card>
      <MarketingConsole base={base} data={data} />
      <AttachmentFeatureSurface>
        <Card as="section" appearance="standard" className="ui-panel">
          <h2>Campaign documents and public creative</h2>
          <p>
            Choose a campaign to manage private sources and separately approved sanitized public
            derivatives.
          </p>
          <div className="ui-form-actions">
            {data.campaigns.map((c) => (
              <UiActionLink appearance="standard"
                variant="secondary"
                key={String(c.id)}
                href={`${base}/marketing?campaignId=${encodeURIComponent(String(c.id))}`}
              >
                {String(c.code)} · {String(c.name)}
              </UiActionLink>
            ))}
          </div>
        </Card>
        {selectedCampaign && (
          <>
            <RecordDocumentPanel appearance="standard"
              base={base}
              targetType="marketing_campaign"
              targetId={String(selectedCampaign.id)}
              expectedTargetVersion={Number(selectedCampaign.version)}
              editable={hasPermission(a, "records.write")}
              title={`${String(selectedCampaign.code)} · campaign files`}
              purposes={[
                {
                  key: "creative",
                  label: "Campaign creative",
                  description:
                    "Attach the private master creative or source image. Publishing creates a separate sanitized derivative.",
                },
                {
                  key: "source",
                  label: "Campaign source",
                  description:
                    "Attach source briefs, production material, or approved channel documents.",
                },
                {
                  key: "consent",
                  label: "Consent evidence",
                  description: "Attach the recorded usage consent or licensing evidence.",
                },
              ]}
            />
            <MediaPublicationPanel appearance="standard"
              base={base}
              targetType="marketing_campaign"
              targetId={String(selectedCampaign.id)}
              purposeKey="creative"
              title={`${String(selectedCampaign.code)} · approved campaign media`}
              canCreate={hasPermission(a, "records.write")}
              canReview={hasPermission(a, "records.review")}
            />
          </>
        )}
        <Card as="section" appearance="standard" className="ui-panel">
          <h2>Marketing asset files</h2>
          <p>
            Select an existing asset to manage its private source, consent evidence, and separately
            approved public derivative.
          </p>
          <div className="ui-form-actions">
            {data.assets
              .filter((asset) => asset.kind === "content")
              .map((asset) => (
                <UiActionLink appearance="standard"
                  variant="secondary"
                  key={String(asset.id)}
                  href={`${base}/marketing?assetId=${encodeURIComponent(String(asset.id))}`}
                >
                  {String(asset.name)} · content
                </UiActionLink>
              ))}
          </div>
        </Card>
        {selectedAsset && (
          <>
            <RecordDocumentPanel appearance="standard"
              base={base}
              targetType="marketing_asset"
              targetId={String(selectedAsset.id)}
              expectedTargetVersion={Number(selectedAsset.version)}
              editable={hasPermission(a, "records.write")}
              title={`${String(selectedAsset.name)} · private asset files`}
              purposes={[
                {
                  key: "creative",
                  label: "Creative master",
                  description:
                    "Attach the private master image or creative source. Public use requires an approved sanitized derivative.",
                },
                {
                  key: "source",
                  label: "Source material",
                  description: "Attach the brief, production source, or channel delivery material.",
                },
                {
                  key: "consent",
                  label: "Consent or licence",
                  description: "Attach the recorded usage consent, licence, or rights evidence.",
                },
              ]}
            />
            <MediaPublicationPanel appearance="standard"
              base={base}
              targetType="marketing_asset"
              targetId={String(selectedAsset.id)}
              purposeKey="creative"
              title={`${String(selectedAsset.name)} · approved public media`}
              canCreate={hasPermission(a, "records.write")}
              canReview={hasPermission(a, "records.review")}
            />
          </>
        )}
      </AttachmentFeatureSurface>
      <Card as="section" appearance="standard" className="ui-panel">
        <h2>Active public forms</h2>
        {data.assets
          .filter((x) => x.kind === "form" && x.status === "active")
          .map((x) => (
            <p key={String(x.id)}>
              <Link
                href={`/forms/${encodeURIComponent(holdingSlug)}/${encodeURIComponent(String(x.id))}`}
              >
                {String(x.name)} · public form
              </Link>
            </p>
          ))}
        {!data.assets.some((x) => x.kind === "form" && x.status === "active") && (
          <p>No active public forms.</p>
        )}
      </Card>
      <Card as="section" appearance="standard" className="ui-panel">
        <h2>Lead candidates</h2>
        {data.candidates.map((x) => (
          <Link className="ui-record-item" key={String(x.id)} href={`${base}/marketing/${x.id}`}>
            {String(x.contact_name)} · {String(x.form_name)} · {String(x.status)}
          </Link>
        ))}
      </Card>
    </PageContent>
  );
}
