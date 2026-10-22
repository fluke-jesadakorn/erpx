import {HandbookNewArticleDrawer} from '@erpx/workplace/components/handbook-new-article-drawer';
import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";
import { collaborationPresentationQueries } from "@erpx/server-collaboration/public/presentation-queries";
import {handbookPresentationContext} from '@erpx/server-collaboration/public/handbook-purchases';

import { ActionLink as UiActionLink } from "@erpx/ui/components/foundation";
import { TimeText } from "@erpx/feature-people_payroll/components/time/time-preference-context";
import { FeaturePage, FeatureTabs,FeatureWidget,WidgetGrid } from "@erpx/feature-shared/components/features/feature-page";
import { PageHeader } from "@erpx/ui/components/primitives";
import { HandbookEditor } from "@erpx/feature-records/components/loaders/workplace--handbook-controls";
import styles from "@erpx/workplace/components/services.module.css";
import { workspaceBase } from "@erpx/feature-work_projection/lib/workspace";
import { publicScope,requireWorkContext } from "@erpx/server-identity_access/public/authorization";

import { accessPage,pageError } from "@erpx/server-work_projection/public/page-error";

import { RecordLink as Link } from "@erpx/feature-records/components/record-link";
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
