import {HandbookNewArticleDrawer} from '@/components/workplace/handbook-new-article-drawer';
import { canonicalUiHref } from "@/lib/work_projection/destination-registry";
import { collaborationPresentationQueries } from "@/server/modules/collaboration/public/presentation-queries";
import {handbookPresentationContext} from '@/server/modules/collaboration/public/handbook-purchases';

import { ActionLink as UiActionLink } from "@/components/ui/foundation";
import { TimeText } from "@/components/people_payroll/time/time-preference-context";
import { FeaturePage, FeatureTabs,FeatureWidget,WidgetGrid } from "@/components/shared/features/feature-page";
import { PageHeader } from "@/components/ui/primitives";
import { HandbookEditor } from "@/components/records/loaders/workplace--handbook-controls";
import styles from "@/components/workplace/services.module.css";
import { workspaceBase } from "@/lib/work_projection/workspace";
import { publicScope,requireWorkContext } from "@/server/modules/identity_access/public/authorization";

import { accessPage,pageError } from "@/server/modules/work_projection/public/page-error";

import { RecordLink as Link } from "@/components/records/record-link";
export default async function Article({
  params,
  searchParams,
}: {
  params: Promise<{ holdingSlug: string; id: string }>;
  searchParams: Promise<{ edit?: string; revision?: string; returnTo?:string }>;
}) {
  const { holdingSlug, id } = await params,
    actor = await requireWorkContext(holdingSlug),
    base = workspaceBase(publicScope(actor)),
    presentation = await collaborationPresentationQueries(actor.holdingId),
    query = await searchParams,
    editing = query.edit === "1",
    historyView = query.revision !== undefined;
  if (id === "new") {
    let access;try{access=await presentation.handbookAccess(actor);}catch(error){return pageError(error,actor.holdingSlug);}
    if (!access.editor||access.purchase&&!access.purchase.newWork) return accessPage("permission", actor.holdingSlug);
    return (
      <HandbookNewArticleDrawer base={base} returnTo={query.returnTo}><FeaturePage className={`content ${styles.stack}`}>
        <Link href={canonicalUiHref(`${base}/handbook?edit=1`)}>← Company handbook</Link>
        <HandbookEditor
          key={`${handbookPresentationContext(actor)}:new`}
          contextKey={handbookPresentationContext(actor)}
          purchase={access.purchase}
          base={base}
          companyId={actor.companyId}
          userId={actor.userId}
        />
      </FeaturePage></HandbookNewArticleDrawer>
    );
  }
  let data;
  try {
    data = await presentation.handbookArticle(actor, id, editing, historyView ? Number(query.revision) : undefined);
  } catch (e) {
    return pageError(e, actor.holdingSlug);
  }
  return (
    <FeaturePage className={`content ${styles.stack}`} data-source="live">
      <Link href={canonicalUiHref(`${base}/handbook${editing ? "?edit=1" : ""}`)}>
        ← Company handbook
      </Link>
      <PageHeader appearance="standard"
        title={data.revision.title || "Untitled draft"}
        description={`${data.revision.category.replaceAll("_", " ")} · Revision ${data.revision.revision}`}
        action={
          data.access.editor &&
          (historyView ||
            !editing ||
            data.article.published_revision !== null) ? (
            <UiActionLink appearance="standard" variant="secondary"

              href={canonicalUiHref(`${base}/handbook/${id}${editing && !historyView ? "" : "?edit=1"}`)}
            >
              {historyView
                ? "Edit latest draft"
                : editing
                  ? "View published article"
                  : "Edit article"}
            </UiActionLink>
          ) : undefined
        }
      /><FeatureTabs/><WidgetGrid pageKey="handbook-detail">
      {editing && !historyView ? (
        <FeatureWidget appearance="standard" id="handbook-detail-article" title="Article"><HandbookEditor
          key={`${handbookPresentationContext(actor)}:${id}`}
          contextKey={handbookPresentationContext(actor)}
          purchase={data.access.purchase}
          base={base}
          companyId={actor.companyId}
          userId={actor.userId}
          id={id}
          version={data.article.version ?? 1}
          revision={data.revision}
          archived={!!data.article.archived}
        /></FeatureWidget>
      ) : (
        <article className={`card ${styles.article}`}>
          {data.revision.body}
        </article>
      )}
      <FeatureWidget appearance="standard" id="handbook-detail-revision-history" title="Revision history"><details className={`card ${styles.details}`}>
        <summary>Revision history</summary>
        {data.history.map((r) => (
          <p key={r.revision}>
            <Link
              href={canonicalUiHref(`${base}/handbook/${id}?${editing ? "edit=1&" : ""}revision=${r.revision}`)}
            >
              Revision {r.revision} · {r.title || "Untitled draft"}
            </Link>{" "}
            · {r.published_at ? "Published" : "Draft"} ·{" "}
            <TimeText appearance="standard" value={r.published_at ?? r.created_at} />
          </p>
        ))}
      </details></FeatureWidget>
    </WidgetGrid></FeaturePage>
  );
}
