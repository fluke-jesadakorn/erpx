import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";
import { peoplePayrollPresentationQueries } from "@erpx/server-people_payroll/public/presentation-queries";

import { FeaturePage, FeatureTabs,FeatureWidget,WidgetGrid } from "@erpx/feature-shared/components/features/feature-page";
import { OvertimeForm } from "@erpx/feature-records/components/loaders/self-service--overtime";
import styles from "@erpx/feature-people_payroll/components/self-service/self-service.module.css";
import { workspaceBase } from "@erpx/feature-work_projection/lib/workspace";
import { publicScope,requireWorkContext } from "@erpx/server-identity_access/public/authorization";
import {getOvertime} from '@erpx/server-people_payroll/public/overtime-read';
import { pageError } from "@erpx/server-work_projection/public/page-error";
import {workforcePresentationContext} from '@erpx/server-people_payroll/public/workforce-presentation';
import {workforcePurchaseControls} from '@erpx/server-people_payroll/public/workforce-purchases';

import { RecordLink as Link } from "@erpx/feature-records/components/record-link";
export default async function OvertimeDetail({params,searchParams}:{params:Promise<{holdingSlug:string;id:string}>;searchParams:Promise<{shift?:string}>}) {
 const {holdingSlug,id}=await params,actor=await requireWorkContext(holdingSlug),base=workspaceBase(publicScope(actor)),record=id==='new'?undefined:await getOvertime(actor,id).catch(e=>pageError(e,holdingSlug));
 const workContext=workforcePresentationContext(actor),purchase=record?record.purchase:await workforcePurchaseControls(actor);let initial={};const {shift}=await searchParams;
 if(id==='new'&&shift){const presentation=await peoplePayrollPresentationQueries(actor.holdingId),staff=await presentation.activeStaff(actor.companyId, actor.userId);const row=await presentation.overtimeRosterShift(shift, actor.companyId, staff.engagement_id);if(row)initial={rosterShiftId:row.id,startLocal:`${row.local_date}T${row.start_time}`,endLocal:row.end_local,timezone:row.timezone};}
 return <FeaturePage className={`content ${styles.workspace}`}><Link href={canonicalUiHref(`${base}/overtime`)}>← My overtime</Link><header className={styles.heading}><div><p className={styles.eyebrow}>Personal request</p><h1>{record?'Overtime request':'Request overtime'}</h1><p>Enter details, review calculated hours, then submit.</p></div></header><FeatureTabs/><WidgetGrid pageKey="overtime-detail"><FeatureWidget id="overtime-request" title="Overtime request"><OvertimeForm key={`${workContext}:${record?.id??'new'}`} base={base} record={record} initial={initial} workContext={workContext} purchase={purchase}/></FeatureWidget></WidgetGrid></FeaturePage>;
}
