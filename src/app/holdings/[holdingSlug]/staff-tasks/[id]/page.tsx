import betaStyles from "@/components/beta/beta.module.css";
import { ApprovalAdminLink } from "@/components/modules/approval-admin-link";
import { readablePost } from "@/server/social-source";
import Link from "next/link";
import { notFound } from "next/navigation";
import {
  TaskEditor,
  TaskActions,
} from "@/components/modules/staff-work-controls";
import { ApprovalHistory } from "@/components/modules/approval-history";
import { staffPage } from "@/server/staff-page";
import { taskDetail } from "@/server/staff-tasks";
import { ApiError } from "@/server/http";
import { StatusBadge } from "@/components/ui/primitives";
export default async function Task({
  params,
  searchParams,
}: {
  params: Promise<{ holdingSlug: string; id: string }>;
  searchParams: Promise<{ return?: string }>;
}) {
  const { holdingSlug, id } = await params,
    { actor, write, base, options, projects } = await staffPage(holdingSlug),
    detail = await taskDetail(write, actor, id).catch((e) => {
      if (e instanceof ApiError && e.status === 404) notFound();
      throw e;
    }),
    t = detail.task;
  const origin = await write
    .prepare("SELECT post_id FROM social_task_origins WHERE task_id=?")
    .bind(id)
    .first<{ post_id: string }>();
  let sourceId: string | null = null;
  if (origin) {
    try {
      await readablePost(write, actor, origin.post_id);
      sourceId = origin.post_id;
    } catch (e) {
      if (!(e instanceof ApiError) || e.status !== 404) throw e;
    }
  }
  const back = await searchParams;
  const returnQuery = new URLSearchParams(
    back.return ?? "queue=all&type=staff_task",
  );
  for (const key of [...returnQuery.keys()])
    if (!["queue", "type", "q"].includes(key)) returnQuery.delete(key);
  return (
    <main className="content-stack" data-source="live">
      <Link className="back-link" href={`${base}/work?${returnQuery}`}>
        ← Your work
      </Link>
      <div className="section-head">
        <h1>{t.title}</h1>
        <StatusBadge status={t.state} />
      </div>
      {sourceId && (
        <Link
          className={`beta-task-link ${betaStyles.surface}`}
          href={`${base}/posts/${sourceId}`}
        >
          Open source conversation ↗
        </Link>
      )}
      <section className="card">
        <dl className="staff-facts">
          <dt>Accountable assignee</dt>
          <dd>{t.assignee_name}</dd>
          <dt>Due</dt>
          <dd>
            {new Date(t.due_at).toISOString().replace("T", " ").slice(0, 16)}{" "}
            UTC
          </dd>
        </dl>
        {t.completion_note && (
          <>
            <h2>Completion note</h2>
            <p style={{ whiteSpace: "pre-wrap" }}>{t.completion_note}</p>
          </>
        )}
      </section>
      <TaskActions key={t.id} base={base} detail={detail} />
      {t.description && (
        <details className="card" open={t.description.length <= 500}>
          <summary>Task instructions</summary>
          <p style={{ whiteSpace: "pre-wrap" }}>{t.description}</p>
        </details>
      )}
      <details className="card">
        <summary>Record information</summary>
        <p>
          Created by {t.creator_name} · {t.project_title ?? "Standalone work"}
        </p>
        <p>
          Revision {t.revision} · version {t.version}
        </p>
      </details>
      <ApprovalHistory
        approval={detail.approval}
        blockedReason={detail.blockedReason}
      />
      {options.canManage &&
        !actor.beta &&
        (["start_approval", "completion_approval"].includes(t.state) ||
          !!detail.blockedReason) && (
          <ApprovalAdminLink base={base} kind="task" id={t.id} />
        )}
      {detail.permittedActions.includes("edit") && (
        <TaskEditor
          key={`edit-${t.version}`}
          base={base}
          options={options}
          projects={projects}
          userId={actor.userId}
          task={t}
        />
      )}{" "}
      {detail.permittedActions.includes("revise") && t.state !== "draft" && (
        <TaskEditor
          key={`revise-${t.version}`}
          base={base}
          options={options}
          projects={projects}
          userId={actor.userId}
          task={t}
          canRevise
        />
      )}
    </main>
  );
}
