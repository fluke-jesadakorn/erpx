import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";
import { legalPresentationQueries } from "@erpx/server-legal/public/presentation-queries";

import { PageContent } from "@erpx/ui/components/patterns";
import { RecordLink as Link } from "@erpx/feature-records/components/record-link";
import { requireWorkContext } from "@erpx/server-identity_access/public/authorization";


import { pageError } from "@erpx/server-work_projection/public/page-error";
import { Card } from "@erpx/ui/components/foundation";
export const dynamic = "force-dynamic";
export default async function CitationPage({
  params,
  searchParams,
}: {
  params: Promise<{ holdingSlug: string; id: string }>;
  searchParams: Promise<{ asOf?: string }>;
}) {
  const { holdingSlug, id } = await params;
  const actor = await requireWorkContext(holdingSlug),
    presentation = await legalPresentationQueries(actor.holdingId);
  // This Next runtime URI-encodes dynamic page params before passing them to
  // user code. Restore the typed source ID once at the transport boundary.
  const source = await presentation.legalCitation(actor, decodeURIComponent(id), (await searchParams).asOf).catch((error) =>
    pageError(error, holdingSlug),
  );
  const base = canonicalUiHref(`/holdings/${holdingSlug}/legal`);
  return (
    <PageContent >
      <Card as="section" appearance="standard" className="ui-panel">
        <Link href={`${base}?record=${encodeURIComponent(source.item_id)}`}>
          Open source record
        </Link>
        <h1>{source.title}</h1>
        <p>
          Immutable version {source.version}
          {source.page ? ` · Page ${source.page}` : ""}
          {source.section ? ` · ${source.section}` : ""}
        </p>
        {"revision_label" in source && (
          <p>
            Law revision {source.revision_label} · Published {source.published_on} · Effective{" "}
            {source.effective_from} · Applicable as of {source.asOf}
          </p>
        )}
        <blockquote style={{ whiteSpace: "pre-wrap", overflowWrap: "anywhere" }}>
          {source.text}
        </blockquote>
        <p>{source.notice}</p>
        <p>
          <a
            href={`/api/v1/holdings/${holdingSlug}/legal/documents/${source.document_id}${source.page ? `#page=${source.page}` : ""}`}
          >
            Download the retained original version
          </a>
          {"originalUrl" in source && (
            <>
              {" "}
              ·{" "}
              <a href={source.originalUrl} target="_blank" rel="noreferrer">
                Official publication
              </a>
            </>
          )}
        </p>
        <details>
          <summary>Source integrity</summary>
          <p style={{ overflowWrap: "anywhere" }}>SHA-256 {source.checksum}</p>
        </details>
      </Card>
    </PageContent>
  );
}
