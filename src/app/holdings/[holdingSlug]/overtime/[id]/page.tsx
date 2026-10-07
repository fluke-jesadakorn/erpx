import { peoplePayrollPresentationQueries } from "@/server/modules/people_payroll/public/presentation-queries";

import { FeaturePage, FeatureTabs,FeatureWidget,WidgetGrid } from "@/components/features/feature-page";
import { OvertimeForm } from "@/components/self-service/overtime";
import styles from "@/components/self-service/self-service.module.css";
import { workspaceBase } from "@/lib/workspace";
import { publicScope,requireWorkContext } from "@/server/modules/identity_access/public/authorization";
import { getOvertime } from "@/server/modules/people_payroll/public/overtime";
import { pageError } from "@/server/modules/work_projection/public/page-error";


import { RecordLink as Link } from "@/components/records/record-link";
export default async function OvertimeDetail({params,searchParams}:{params:Promise<{holdingSlug:string;id:string}>;searchParams:Promise<{shift?:string}>}) {
 const {holdingSlug,id}=await params,actor=await requireWorkContext(holdingSlug),base=workspaceBase(publicScope(actor)),record=id==='new'?undefined:await getOvertime(actor,id).catch(e=>pageError(e,holdingSlug));
 let initial={};const {shift}=await searchParams;
 if(id==='new'&&shift){const presentation=await peoplePayrollPresentationQueries(actor.holdingId),staff=await presentation.activeStaff(actor.companyId, actor.userId);const row=await presentation.overtimeRosterShift(shift, actor.companyId, staff.engagement_id);if(row)initial={rosterShiftId:row.id,startLocal:`${row.local_date}T${row.start_time}`,endLocal:row.end_local,timezone:row.timezone};}
 return <FeaturePage className={`content ${styles.workspace}`}><Link href={`${base}/overtime`}>← My overtime</Link><header className={styles.heading}><div><p className={styles.eyebrow}>Personal request</p><h1>{record?'Overtime request':'Request overtime'}</h1><p>Enter details, review calculated hours, then submit.</p></div></header><FeatureTabs/><WidgetGrid pageKey="overtime-detail"><FeatureWidget id="overtime-request" title="Overtime request"><OvertimeForm key={`${actor.companyId}:${actor.userId}:${record?.id??'new'}`} base={base} record={record} initial={initial}/></FeatureWidget></WidgetGrid></FeaturePage>;
}
