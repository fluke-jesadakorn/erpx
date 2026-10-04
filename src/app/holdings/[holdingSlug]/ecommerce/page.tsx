import Link from "next/link";
import { requireWorkContext, publicScope } from "@/server/authorization";
import { workspaceBase } from "@/lib/workspace";
import { pageError } from "@/server/page-error";
import { ecommerceWorkspace } from "@/server/ecommerce";
import { EcommerceConsole } from "@/components/modules/ecommerce-console";
import { RecordDocumentDrawer } from "@/components/attachments/record-document-drawer";
import { MediaPublicationPanel } from "@/components/attachments/media-publication-panel";
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
    actor = await requireWorkContext(holdingSlug),
    base = workspaceBase(publicScope(actor));
  const data = await ecommerceWorkspace(actor).catch((error) =>
    pageError(error, holdingSlug),
  );
  return (
    <PageContent as="div" className="ui-page-content">
      <Link href={`${base}/commerce`}>Sales and stock</Link>
      <h1>E-commerce</h1>
      <p>
        Approve sellable channels and listings, lock the customer cart into a
        Sales order, reserve stock, and trace fulfillment and returns to their
        owner records.
      </p>
      <EcommerceConsole base={base} data={data} />
      {data.listings.map((listing) => (
        <AttachmentFeatureSurface key={`files-${String(listing.id)}`}>
          <Card as="section" appearance="standard" className="ui-panel">
            <div className="ui-section-header">
              <div>
                <h2>{String(listing.title)}</h2>
                <p className="muted">
                  Listing files · version {String(listing.version)}
                </p>
              </div>
              <RecordDocumentDrawer
                base={base}
                targetType="ecommerce_listing"
                targetId={String(listing.id)}
                expectedTargetVersion={Number(listing.version)}
                editable={hasPermission(actor, "commerce.sales")}
                title="Listing media and channel documents"
                triggerLabel="Private files"
                purposes={[
                  {
                    key: "listing_media",
                    label: "Listing media",
                    description:
                      "Attach private product or storefront source images. Public use requires a separately approved sanitized publication.",
                  },
                  {
                    key: "channel_document",
                    label: "Channel document",
                    description:
                      "Attach channel terms, listing instructions, or marketplace correspondence.",
                  },
                  {
                    key: "compliance_document",
                    label: "Compliance document",
                    description:
                      "Attach the reviewed channel or product compliance document for this listing.",
                  },
                ]}
              />
            </div>
            <details>
              <summary>Approved public listing media</summary>
              <MediaPublicationPanel appearance="standard"
                base={base}
                targetType="ecommerce_listing"
                targetId={String(listing.id)}
                purposeKey="listing_media"
                title="Approved public listing media"
                canCreate={hasPermission(actor, "commerce.sales")}
                canReview={hasPermission(actor, "commerce.approve")}
              />
            </details>
          </Card>
        </AttachmentFeatureSurface>
      ))}
      <Card as="section" appearance="standard" className="ui-panel">
        <h2>Checkout handoffs</h2>
        {data.checkouts.map((x) => (
          <Link
            className="ui-record-item"
            key={String(x.id)}
            href={`${base}/ecommerce/${x.id}`}
          >
            {String(x.number)} · {String(x.status)} · Sales{" "}
            {String(x.order_state)}
          </Link>
        ))}
      </Card>
    </PageContent>
  );
}
