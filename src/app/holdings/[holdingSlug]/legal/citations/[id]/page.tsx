import Link from "next/link";
import { requireWorkContext } from "@/server/authorization";
import { holdingResources } from "@/server/runtime";
import { legalCitation } from "@/server/legal/citations";
export const dynamic = "force-dynamic";
export default async function CitationPage({ params, searchParams }: { params: Promise<{ holdingSlug: string; id: string }>; searchParams: Promise<{ asOf?: string }> }) {
  const { holdingSlug, id } = await params;
  const actor = await requireWorkContext(holdingSlug), { write } = await holdingResources(actor.holdingId);
  const source = await legalCitation(write, actor, id, (await searchParams).asOf);
  const base = `/holdings/${holdingSlug}/legal`;
  return <main className="content"><section className="card">
    <Link href={`${base}?record=${encodeURIComponent(source.item_id)}`}>Open source record</Link>
    <h1>{source.title}</h1><p>Immutable version {source.version}{source.page ? ` · Page ${source.page}` : ""}{source.section ? ` · ${source.section}` : ""}</p>
    {"revision_label" in source && <p>Law revision {source.revision_label} · Published {source.published_on} · Effective {source.effective_from} · Applicable as of {source.asOf}</p>}
    <blockquote style={{ whiteSpace: "pre-wrap", overflowWrap: "anywhere" }}>{source.text}</blockquote>
    <p>{source.notice}</p><p><a href={`/api/v1/holdings/${holdingSlug}/legal/documents/${source.document_id}${source.page ? `#page=${source.page}` : ""}`}>Download the retained original version</a>{"originalUrl" in source && <> · <a href={source.originalUrl} target="_blank" rel="noreferrer">Official publication</a></>}</p>
    <details><summary>Source integrity</summary><p style={{ overflowWrap: "anywhere" }}>SHA-256 {source.checksum}</p></details>
  </section></main>;
}
