import { TimeText } from "@/components/time/time-preference-context";
import betaStyles from "@/components/beta/beta.module.css";
import { FeaturePage, FeatureTabs,FeatureWidget,WidgetGrid } from "@/components/features/feature-page";
import { ApprovalAdminLink } from "@/components/modules/approval-admin-link";
import { ApprovalHistory } from "@/components/modules/approval-history";
import {
TaskActions,
TaskEditor,
TaskDelegationControls,
} from "@/components/modules/staff-work-controls";
import { StatusBadge } from "@/components/ui/primitives";
import { FlowPanel,documentFlow } from "@/components/workplace/flow-panel";
import { RecordNextAction } from "@/components/workplace/record-next-action";
import { WorkReturn } from "@/components/workplace/work-return";
import { ApiError } from "@/server/http";
import { pageError } from "@/server/page-error";
import { readablePost } from "@/server/social-source";
import { staffPage } from "@/server/staff-page";
import { taskDetail } from "@/server/staff-tasks";
import Link from "next/link";
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
      return pageError(e, holdingSlug);
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
  const flow = await documentFlow(actor,"staff_task",id);
  return (
    <FeaturePage className="content-stack" data-source="live"><WorkReturn base={base}/>
      <Link className="back-link" href={`${base}/work?${returnQuery}`}>
        ← Your work
      </Link>
      <div className="section-head">
        <h1>{t.title}</h1>
        <StatusBadge status={t.state} />
      </div><FeatureTabs/><WidgetGrid pageKey="staff-tasks-detail">
      {sourceId && (
        <Link
          className={`beta-task-link ${betaStyles.surface}`}
          href={`${base}/posts/${sourceId}`}
        >
          Open source conversation ↗
        </Link>
      )}
      <section data-critical-notice><RecordNextAction flow={flow}/></section><TaskActions key={t.id} base={base} detail={{...detail,permittedActions:detail.permittedActions.filter(a=>flow.canAct||detail.task.active_delegate_user_id===actor.userId||["edit","revise","cancel","delegate"].includes(a))}} /><TaskDelegationControls base={base} detail={detail} options={options} /><FeatureWidget id="staff-tasks-detail-completion-note" title="Completion note">
        <dl className="staff-facts">
          <dt>Accountable assignee</dt>
          <dd>{t.assignee_name}</dd>
          <dt>Due</dt>
          <dd>
            {t.due_at === null
              ? "No due date"
              : <TimeText value={t.due_at} />}
          </dd>
        </dl>
        {t.completion_note && (
          <>
            <h2>Completion note</h2>
            <p style={{ whiteSpace: "pre-wrap" }}>{t.completion_note}</p>
          </>
        )}
      </FeatureWidget>

      {t.description && (
        <FeatureWidget id="staff-tasks-detail-task-instructions" title="Task instructions"><details className="card" open={t.description.length <= 500}>
          <summary>Task instructions</summary>
          <p style={{ whiteSpace: "pre-wrap" }}>{t.description}</p>
        </details></FeatureWidget>
      )}
      <FeatureWidget id="staff-tasks-detail-record-information" title="Record information"><details className="card">
        <summary>Record information</summary>
        <p>
          Created by {t.creator_name} · {t.project_title ?? "Standalone work"}
        </p>
        <p>
          Revision {t.revision} · version {t.version}
        </p>
      </details></FeatureWidget>

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
    <ApprovalHistory
        approval={detail.approval}
        blockedReason={detail.blockedReason}
      />
      <FlowPanel flow={flow} actor={actor} kind="staff_task" id={id} base={base}/></WidgetGrid></FeaturePage>
  );
}
