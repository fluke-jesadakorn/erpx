import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";
import { peoplePayrollPresentationQueries } from "@erpx/server-people_payroll/public/presentation-queries";
import { Card } from "@erpx/ui/components/foundation";

import { ActionLink as UiActionLink } from "@erpx/ui/components/foundation";
import { FeaturePage, FeatureTabs,FeatureWidget,WidgetGrid } from "@erpx/feature-shared/components/features/feature-page";
import styles from "@erpx/feature-people_payroll/components/self-service/self-service.module.css";
import { overtimeLabels } from "@erpx/feature-people_payroll/lib/overtime";
import { workspaceBase } from "@erpx/feature-work_projection/lib/workspace";
import { publicScope,requireWorkContext } from "@erpx/server-identity_access/public/authorization";
import { ApiError } from "@erpx/server-runtime/public/http";
import {listOvertime} from '@erpx/server-people_payroll/public/overtime';

import { RecordLink as Link } from "@erpx/feature-records/components/record-link";
import {workforcePurchaseControls} from "@erpx/server-people_payroll/public/workforce-purchases";
export default async function Overtime({params}:{params:Promise<{holdingSlug:string}>}) {
 const actor=await requireWorkContext((await params).holdingSlug),base=workspaceBase(publicScope(actor)),presentation=await peoplePayrollPresentationQueries(actor.holdingId);
 const purchase=await workforcePurchaseControls(actor);
 const [requests,setup]=await Promise.all([listOvertime(actor),presentation.overtimeSetup(actor).then(s=>({blockers:s.blockers,canDraft:true})).catch(e=>{if(e instanceof ApiError)return {blockers:[e.message],canDraft:false};throw e;})]);
 return <FeaturePage className={`content ${styles.workspace}`}><Link href={canonicalUiHref(`${base}/home`)}>← Home</Link><header className={styles.heading}><div><p className={styles.eyebrow}>My work · Self-service</p><h1>Overtime</h1><p>Plan extra work, submit actual hours, and follow each review.</p></div>{purchase?.newWork!==false&&<UiActionLink appearance="standard" variant="primary" href={canonicalUiHref(`${base}/overtime/new`)}>Request overtime</UiActionLink>}</header><FeatureTabs/><WidgetGrid pageKey="overtime">{!!setup.blockers.length&&<FeatureWidget appearance="standard" id="overtime-setup-needs-attention" title="Setup needs attention"><Card as="section" appearance="standard" className={styles.panel} role="status">{setup.blockers.map(b=><p key={b}>{b}</p>)}<p>Ask HR to resolve this in <Link href={canonicalUiHref(`${base}/people/settings?section=leave`)}>People settings</Link>. {setup.canDraft?'You can still save a draft.':'An active staff engagement is needed before creating a request.'}</p></Card></FeatureWidget>}<FeatureWidget appearance="standard" id="overtime-requests-history" title="Requests & history"><Card as="section" appearance="standard" className={styles.panel}>{requests.length?requests.map(r=><Link className={styles.row} key={r.id} href={canonicalUiHref(`${base}/overtime/${r.id}`)}><span><strong>{r.start_local.slice(0,10)} · {(r.minutes/60).toFixed(2)} hours</strong><small>{r.mode==='advance'?'Advance request':'Retrospective claim'} · {r.approval.currentApprover?.approver_name??'Requester'}</small></span><span className={styles.status}>{overtimeLabels[r.state]}</span></Link>):<p className={styles.muted}>Your requests will appear here, including saved drafts.</p>}</Card></FeatureWidget><p className={styles.muted}>Final approval supplies hours to payroll preview. It does not calculate statutory pay or authorize payment.</p></WidgetGrid></FeaturePage>;
}
