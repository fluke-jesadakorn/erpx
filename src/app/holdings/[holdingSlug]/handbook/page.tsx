import { pageError } from "@/server/page-error";
import Link from "next/link";
import { publicScope, requireWorkContext } from "@/server/authorization";
import { holdingResources } from "@/server/runtime";
import { workspaceBase } from "@/lib/workspace";
import { handbookList, handbookEditorOptions } from "@/server/handbook";
import { companyMembers } from "@/server/member-directory";
import { HandbookEditors } from "@/components/workplace/handbook-controls";
import { PageHeader } from "@/components/ui/primitives";
import styles from "@/components/workplace/services.module.css";
export default async function Handbook({
  params,
  searchParams,
}: {
  params: Promise<{ holdingSlug: string }>;
  searchParams: Promise<{ q?: string; edit?: string }>;
}) {
  const actor = await requireWorkContext((await params).holdingSlug),
    base = workspaceBase(publicScope(actor)),
    { write } = await holdingResources(actor.holdingId),
    query = await searchParams,
    editing = query.edit === "1",
    data = await handbookList(write, actor, query.q ?? "", editing).catch(
      (e: unknown) => {
        return pageError(e, actor.holdingSlug);
      },
    );
  return (
    <main className={`content ${styles.stack}`} data-source="live">
      <PageHeader
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
      {data.access.editor && (
        <nav className={styles.actions}>
          <Link
            className="button secondary"
            href={`${base}/handbook`}
            aria-current={!editing ? "page" : undefined}
          >
            Published
          </Link>
          <Link
            className="button secondary"
            href={`${base}/handbook?edit=1`}
            aria-current={editing ? "page" : undefined}
          >
            Drafts & editing
          </Link>
        </nav>
      )}
      <form className={styles.form} action={`${base}/handbook`}>
        <label>
          Search handbook
          <input
            name="q"
            type="search"
            defaultValue={query.q}
            placeholder="Find a policy, procedure, or answer"
          />
        </label>
        {editing && <input name="edit" type="hidden" value="1" />}
        <button className="button secondary">Search</button>
      </form>
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
        {!data.articles.length && (
          <p>
            {query.q
              ? "No articles match your search."
              : "No articles published yet. Company guidance will appear here."}
          </p>
        )}
      </section>
      {editing && data.access.owner && (
        <HandbookEditors
          base={base}
          companyId={actor.companyId}
          userId={actor.userId}
          options={await handbookEditorOptions(write, actor)}
          members={await companyMembers(actor)}
        />
      )}
    </main>
  );
}
