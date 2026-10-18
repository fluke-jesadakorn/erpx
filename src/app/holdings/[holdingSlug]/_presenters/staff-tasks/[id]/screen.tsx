import {ProjectPurchaseProvider} from "@/components/projects/purchase-context";
import { staffPickerDisplayOptions } from "@/lib/work_projection/staff-picker-display";
import { canonicalUiHref } from "@/lib/work_projection/destination-registry";
import { staffPresentationQueries } from "@/server/modules/work_projection/public/presentation-context-queries";

import { TimeText } from "@/components/people_payroll/time/time-preference-context";
import betaStyles from "@/components/collaboration/beta.module.css";
import { FeaturePage, FeatureTabs,FeatureWidget,WidgetGrid } from "@/components/shared/features/feature-page";
import { ApprovalAdminLink } from "@/components/workflow/approval-admin-link";
import { ApprovalHistory } from "@/components/workflow/approval-history";
import { TaskActions, TaskEditor, TaskDelegationControls } from "@/components/records/loaders/modules--staff-work-controls";
import { StatusBadge } from "@/components/ui/primitives";
import { FlowPanel,documentFlow } from "@/components/workplace/flow-panel";
import { RecordNextAction } from "@/components/workplace/record-next-action";
import { WorkReturn } from "@/components/records/loaders/workplace--work-return";
import { ApiError } from "@/server/modules/runtime/public/http";
import { pageError } from "@/server/modules/work_projection/public/page-error";



import { RecordLink as Link } from "@/components/records/record-link";
export default async function Task({
  params,
  searchParams,
}: {
  params: Promise<{ holdingSlug: string; id: string }>;
  searchParams: Promise<{ return?: string }>;
}) {
  const { holdingSlug, id } = await params,
    { actor, queries, base, options, projects } = await staffPresentationQueries(holdingSlug,{projectOptions:'optional'}),
    detail = await queries.taskDetail(actor, id).catch((e) => {
      return pageError(e, holdingSlug);
    }),
    t = detail.task;
  const purchases=t.project_id||projects.length?await queries.projectPurchases():undefined;
  const pickerOptions=staffPickerDisplayOptions(options);
  const origin = await queries.taskSocialOrigin(id);
  let sourceId: string | null = null;
  if (origin) {
    try {
      await queries.readablePost(actor, origin.post_id);
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
    <ProjectPurchaseProvider purchases={purchases}><FeaturePage className="ui-page-content" data-source="live"><WorkReturn base={base}/>
      <Link className="back-link" href={canonicalUiHref(`${base}/work?${returnQuery}`)}>
        ← Your work
      </Link>
      <div className="ui-section-header">
        <h1>{t.title}</h1>
        <StatusBadge appearance="standard" status={t.state} />
      </div><FeatureTabs/><WidgetGrid pageKey="staff-tasks-detail">
      {sourceId && (
        <Link
          className={`beta-task-link ${betaStyles.surface}`}
          href={canonicalUiHref(`${base}/posts/${sourceId}`)}
        >
          Open source conversation ↗
        </Link>
      )}
      <section data-critical-notice><RecordNextAction flow={flow}/></section><TaskActions appearance="standard" key={t.id} base={base} detail={{...detail,permittedActions:detail.permittedActions.filter(a=>flow.canAct||detail.task.active_delegate_user_id===actor.userId||["edit","revise","cancel","delegate"].includes(a))}} /><TaskDelegationControls appearance="standard" base={base} detail={detail} options={pickerOptions} /><FeatureWidget appearance="standard" id="staff-tasks-detail-completion-note" title="Completion note">
        <dl className="staff-facts">
          <dt>Accountable assignee</dt>
          <dd>{t.assignee_name}</dd>
          <dt>Due</dt>
          <dd>
            {t.due_at === null
              ? "No due date"
              : <TimeText appearance="standard" value={t.due_at} />}
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
        <FeatureWidget appearance="standard" id="staff-tasks-detail-task-instructions" title="Task instructions"><details className="ui-panel" open={t.description.length <= 500}>
          <summary>Task instructions</summary>
          <p style={{ whiteSpace: "pre-wrap" }}>{t.description}</p>
        </details></FeatureWidget>
      )}
      <FeatureWidget appearance="standard" id="staff-tasks-detail-record-information" title="Record information"><details className="ui-panel">
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
      <ProjectPurchaseProvider purchases={purchases} editorSource={{companyId:actor.companyId,actorId:actor.userId,available:detail.permittedActions.includes("edit")||detail.permittedActions.includes("revise")&&t.state!=="draft",record:{kind:"task",id:t.id,version:Number(t.version),projectId:t.project_id,state:t.state}}}>
      {(detail.permittedActions.includes("edit")||detail.permittedActions.includes("revise")&&t.state!=="draft") && (
        <TaskEditor appearance="standard"
          key={t.project_id?`task-editor-${t.id}`:detail.permittedActions.includes("edit")?`edit-${t.version}`:`revise-${t.version}`}
          base={base}
          options={pickerOptions}
          projects={projects.map(({id,title})=>({id,title}))}
          userId={actor.userId}
          companyId={actor.companyId}
          assignmentId={actor.assignmentId}
          task={t}
          canRevise={!detail.permittedActions.includes("edit")&&detail.permittedActions.includes("revise")&&t.state!=="draft"}
        />
      )}
      </ProjectPurchaseProvider>
    <ApprovalHistory
        approval={detail.approval}
        blockedReason={detail.blockedReason}
      />
      <FlowPanel flow={flow} actor={actor} kind="staff_task" id={id} base={base}/></WidgetGrid></FeaturePage></ProjectPurchaseProvider>
  );
}
