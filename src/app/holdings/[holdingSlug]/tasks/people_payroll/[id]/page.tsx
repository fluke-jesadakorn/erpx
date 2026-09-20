import { FeaturePage, FeatureTabs,FeatureWidget,WidgetGrid } from "@/components/features/feature-page";
import { ApprovalHistory } from "@/components/modules/approval-history";
import { LeaveDecisions,LeaveEditor } from "@/components/self-service/leave-editor";
import styles from "@/components/self-service/self-service.module.css";
import { workspaceBase } from "@/lib/workspace";
import { publicScope,requireWorkContext } from "@/server/authorization";
import { getLeave } from "@/server/leave";
import { leaveSummary } from "@/server/leave-self-service";
import { pageError } from "@/server/page-error";
import { getLeaveEvidence } from "@/server/work";
import Link from "next/link";
export default async function LeaveTaskPage({params}: {params:Promise<{holdingSlug:string;id:string}>}) {
 const {holdingSlug,id}=await params,actor=await requireWorkContext(holdingSlug),base=workspaceBase(publicScope(actor)),request=await getLeave(actor.holdingId,id,actor).catch(e=>pageError(e,holdingSlug));
 const evidence=await getLeaveEvidence(actor.holdingId,id,actor),editable=request.permittedActions.includes('save'),summary=editable?await leaveSummary(actor):null;
 const approval=request.cancellationApproval??request.approval;
 return <FeaturePage className={`content ${styles.workspace}`}><Link href={`${base}/leave`}>← My leave</Link><header className={styles.heading}><div><p className={styles.eyebrow}>Personal request</p><h1>Leave request</h1><p>{request.start_date} – {request.end_date??request.start_date} · {request.calculated_units??request.days} working days</p></div><span className={styles.status}>{request.cancellation_state==='pending_review'?'Cancellation in review':request.state.replaceAll('_',' ')}</span></header><FeatureTabs/><WidgetGrid pageKey="tasks-people-payroll-detail">
 <section className={styles.panel}><h2>{request.cancellation_state==='pending_review'?'Cancellation review':editable?'Prepare and submit':request.state==='pending_approval'?'Approval in progress':request.state.replaceAll('_',' ')}</h2><p>Responsible: {approval.currentApprover?.approver_name??(editable?'Requester':'No pending review')}</p>{approval.blockedReason&&<p role="status">{approval.blockedReason} Ask HR to resolve this in People settings.</p>}{request.cancellation_state==='pending_review'&&<p>Approved absence and usage remain effective until both the manager and independent HR approve cancellation.</p>}{request.decision_note&&<p><strong>Decision reason:</strong> {request.decision_note}</p>}{(approval.instance||approval.history.length>0)&&<ApprovalHistory approval={approval}/>}{request.cancellationApproval&&<FeatureWidget id="work-original-approval-history" title="Original approval history"><details><summary>Original approval history</summary><ApprovalHistory approval={request.approval}/></details></FeatureWidget>}</section>
 {!editable&&<LeaveDecisions key={`${id}:${request.version}`} base={base} record={request}/>}
 {editable&&summary?<FeatureWidget id="work-leave-request" title="Leave request"><LeaveEditor key={`${actor.companyId}:${actor.userId}:${id}`} base={base} record={request} types={summary.balances}/></FeatureWidget>:<FeatureWidget id="work-request-details" title="Request details"><section className={styles.panel}><p>{request.start_segment?.replaceAll('_',' ')} → {request.end_segment?.replaceAll('_',' ')}</p><p>{request.handover_note||'No handover note added.'}</p><p className={styles.muted}>Submitted calculations and policy terms are frozen with this request.</p></section></FeatureWidget>}
 {editable&&<LeaveDecisions key={`${id}:${request.version}`} base={base} record={request}/>}
 <FeatureWidget id="work-private-supporting-evidence" title="Private supporting evidence"><section className={styles.panel}>{evidence.length?evidence.map(f=><a className={styles.row} key={f.id} href={`/api/v1${base}/attachments/${f.id}/content`}>{f.original_name} · Download</a>):<p>No evidence attached.</p>}</section></FeatureWidget>
 </WidgetGrid></FeaturePage>;
}
