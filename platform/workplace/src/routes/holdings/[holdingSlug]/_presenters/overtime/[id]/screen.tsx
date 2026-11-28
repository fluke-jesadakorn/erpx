import { canonicalUiHref } from "@erpx/work-projection-ui/lib/destination-registry";
import { peoplePayrollPresentationQueries } from "@erpx/people-payroll-server/public/presentation-queries";

import { FeaturePage, FeatureTabs,FeatureWidget,WidgetGrid } from "@erpx/platform-shared-ui/components/features/feature-page";
import { OvertimeForm } from "@erpx/records-ui/components/loaders/self-service--overtime";
import styles from "@erpx/people-payroll-ui/components/self-service/self-service.module.css";
import { workspaceBase } from "@erpx/work-projection-ui/lib/workspace";
import { publicScope,requireWorkContext } from "@erpx/identity-access-server/public/authorization";
import {getOvertime} from '@erpx/people-payroll-server/public/overtime-read';
import { pageError } from "@erpx/work-projection-server/public/page-error";
import {workforcePresentationContext} from '@erpx/people-payroll-server/public/workforce-presentation';
import {workforcePurchaseControls} from '@erpx/people-payroll-server/public/workforce-purchases';

import { RecordLink as Link } from "@erpx/records-ui/components/record-link";
export default async function OvertimeDetail({params,searchParams}:{params:Promise<{holdingSlug:string;id:string}>;searchParams:Promise<{shift?:string}>}) {
 const {holdingSlug,id}=await params,actor=await requireWorkContext(holdingSlug),base=workspaceBase(publicScope(actor)),record=id==='new'?undefined:await getOvertime(actor,id).catch(e=>pageError(e,holdingSlug));
 const workContext=workforcePresentationContext(actor),purchase=record?record.purchase:await workforcePurchaseControls(actor);let initial={};const {shift}=await searchParams;
 if(id==='new'&&shift){const presentation=await peoplePayrollPresentationQueries(actor.holdingId),staff=await presentation.activeStaff(actor.companyId, actor.userId);const row=await presentation.overtimeRosterShift(shift, actor.companyId, staff.engagement_id);if(row)initial={rosterShiftId:row.id,startLocal:`${row.local_date}T${row.start_time}`,endLocal:row.end_local,timezone:row.timezone};}
 return <FeaturePage className={`content ${styles.workspace}`}><Link href={canonicalUiHref(`${base}/overtime`)}>← My overtime</Link><header className={styles.heading}><div><p className={styles.eyebrow}>Personal request</p><h1>{record?'Overtime request':'Request overtime'}</h1><p>Enter details, review calculated hours, then submit.</p></div></header><FeatureTabs/><WidgetGrid pageKey="overtime-detail"><FeatureWidget id="overtime-request" title="Overtime request"><OvertimeForm key={`${workContext}:${record?.id??'new'}`} base={base} record={record} initial={initial} workContext={workContext} purchase={purchase}/></FeatureWidget></WidgetGrid></FeaturePage>;
}
