import Link from "next/link";
import { publicScope, requireWorkContext } from "@/server/authorization";
import { holdingResources } from "@/server/runtime";
import { workspaceBase } from "@/lib/workspace";
import { handbookArticle, handbookAccess } from "@/server/handbook";
import { HandbookEditor } from "@/components/workplace/handbook-controls";
import { pageError, accessPage } from "@/server/page-error";
import { PageHeader } from "@/components/ui/primitives";
import styles from "@/components/workplace/services.module.css";
export default async function Article({
  params,
  searchParams,
}: {
  params: Promise<{ holdingSlug: string; id: string }>;
  searchParams: Promise<{ edit?: string; revision?: string }>;
}) {
  const { holdingSlug, id } = await params,
    actor = await requireWorkContext(holdingSlug),
    base = workspaceBase(publicScope(actor)),
    { write } = await holdingResources(actor.holdingId),
    query = await searchParams,
    editing = query.edit === "1",
    historyView = query.revision !== undefined;
  if (id === "new") {
    if (!(await handbookAccess(write, actor)).editor) return accessPage("permission", actor.holdingSlug);
    return (
      <main className={`content ${styles.stack}`}>
        <Link href={`${base}/handbook?edit=1`}>← Company handbook</Link>
        <HandbookEditor
          base={base}
          companyId={actor.companyId}
          userId={actor.userId}
        />
      </main>
    );
  }
  let data;
  try {
    data = await handbookArticle(
      write,
      actor,
      id,
      editing,
      historyView ? Number(query.revision) : undefined,
    );
  } catch (e) {
    return pageError(e, actor.holdingSlug);
  }
  return (
    <main className={`content ${styles.stack}`} data-source="live">
      <Link href={`${base}/handbook${editing ? "?edit=1" : ""}`}>
        ← Company handbook
      </Link>
      <PageHeader
        title={data.revision.title || "Untitled draft"}
        description={`${data.revision.category.replaceAll("_", " ")} · Revision ${data.revision.revision}`}
        action={
          data.access.editor &&
          (historyView ||
            !editing ||
            data.article.published_revision !== null) ? (
            <Link
              className="button secondary"
              href={`${base}/handbook/${id}${editing && !historyView ? "" : "?edit=1"}`}
            >
              {historyView
                ? "Edit latest draft"
                : editing
                  ? "View published article"
                  : "Edit article"}
            </Link>
          ) : undefined
        }
      />
      {editing && !historyView ? (
        <HandbookEditor
          base={base}
          companyId={actor.companyId}
          userId={actor.userId}
          id={id}
          version={data.article.version ?? 1}
          revision={data.revision}
          archived={!!data.article.archived}
        />
      ) : (
        <article className={`card ${styles.article}`}>
          {data.revision.body}
        </article>
      )}
      <details className={`card ${styles.details}`}>
        <summary>Revision history</summary>
        {data.history.map((r) => (
          <p key={r.revision}>
            <Link
              href={`${base}/handbook/${id}?${editing ? "edit=1&" : ""}revision=${r.revision}`}
            >
              Revision {r.revision} · {r.title || "Untitled draft"}
            </Link>{" "}
            · {r.published_at ? "Published" : "Draft"} ·{" "}
            {new Date(r.published_at ?? r.created_at).toLocaleDateString(
              "en-GB",
              { timeZone: "Asia/Bangkok" },
            )}
          </p>
        ))}
      </details>
    </main>
  );
}
