import { FeaturePage, FeatureWidget,WidgetGrid } from "@/components/features/feature-page";
import { PeopleNavigation } from "@/components/modules/people-navigation";
import { ListFilters } from "@/components/ui/list-filters";
import { ListPagination } from "@/components/ui/list-pagination";
import { EmptyState,PageHeader } from "@/components/ui/primitives";
import { HandbookEditors } from "@/components/workplace/handbook-controls";
import styles from "@/components/workplace/services.module.css";
import { workspaceBase } from "@/lib/workspace";
import { publicScope,requireWorkContext } from "@/server/authorization";
import { handbookEditorOptions,handbookList } from "@/server/handbook";
import { companyMembers } from "@/server/member-directory";
import { pageError } from "@/server/page-error";
import { holdingResources } from "@/server/runtime";
import Link from "next/link";
export default async function Handbook({
  params,
  searchParams,
}: {
  params: Promise<{ holdingSlug: string }>;
  searchParams: Promise<{ q?: string; edit?: string;cursor?:string; "drafts.cursor"?:string }>;
}) {
  const actor = await requireWorkContext((await params).holdingSlug),
    base = workspaceBase(publicScope(actor)),
    { write } = await holdingResources(actor.holdingId),
    query = await searchParams,
    editing = false,
    data = await handbookList(write, actor, query.q ?? "", editing,query.cursor).catch(
      (e: unknown) => {
        return pageError(e, actor.holdingSlug);
      },
    );
  const drafts = data.access.editor ? await handbookList(write, actor, query.q ?? "", true, query["drafts.cursor"]) : null;
  return (
    <FeaturePage className={`content ${styles.stack}`} data-source="live">
      <PageHeader
        eyebrow={actor.beta ? "Resources" : "People"}
        title="Company handbook"
        description="Policies, procedures, answers, and guidance for your workplace."
        action={
          data.access.editor ? (
            <Link className="button" href={`${base}/handbook/new`}>
              New article
            </Link>
          ) : undefined
        }
      />
      {!actor.beta && <PeopleNavigation base={base} actor={actor} scopeKind={actor.scope.kind} active="handbook" />}
      <WidgetGrid pageKey="handbook">
      <FeatureWidget id="handbook-published" title="Published knowledge">      <ListFilters action={`${base}/handbook`} query={query.q} placeholder="Search handbook" preserve={editing ? {edit:"1"} : {}}/>
      <section className="card">
        {data.articles.map((article) => (
          <article className={styles.history} key={article.id}>
            <small className={styles.quiet}>
              {article.category.replaceAll("_", " ")}
              {article.archived
                ? " · Archived"
                : editing && !article.published_at
                  ? " · Draft"
                  : ""}
            </small>
            <h2>
              <Link
                href={`${base}/handbook/${article.id}${editing ? "?edit=1" : ""}`}
              >
                {article.title || "Untitled draft"}
              </Link>
            </h2>
            <p>{article.excerpt}</p>
          </article>
        ))}
        {!data.articles.length && <EmptyState title={query.q?"No matching articles":"No articles published yet"} description={query.q?"Try a different search or clear filters.":"Company policies and guidance will appear here."}/>}
      </section>
<ListPagination {...data} query={query} count={data.articles.length}/></FeatureWidget>
      {drafts && <FeatureWidget id="handbook-drafts" title="Drafts & editing">{drafts.articles.map(article=><article key={article.id}><h3><Link href={`${base}/handbook/${article.id}?edit=1`}>{article.title || "Untitled draft"}</Link></h3><p>{article.excerpt}</p></article>)}{!drafts.articles.length && <p>No drafts.</p>}<ListPagination {...drafts} cursorName="drafts.cursor" query={query} count={drafts.articles.length}/></FeatureWidget>}
      {data.access.owner && (<FeatureWidget id="handbook-editors" title="Editorial access">
        <HandbookEditors
          base={base}
          companyId={actor.companyId}
          userId={actor.userId}
          options={await handbookEditorOptions(write, actor)}
          members={await companyMembers(actor)}
        /></FeatureWidget>
      )}
    </WidgetGrid></FeaturePage>
  );
}
