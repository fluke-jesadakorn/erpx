import { FeaturePage, FeatureTabs,FeatureWidget,WidgetGrid } from "@/components/features/feature-page";
import { OvertimeForm } from "@/components/self-service/overtime";
import styles from "@/components/self-service/self-service.module.css";
import { workspaceBase } from "@/lib/workspace";
import { publicScope,requireWorkContext } from "@/server/authorization";
import { getOvertime } from "@/server/overtime";
import { pageError } from "@/server/page-error";
import { holdingResources } from "@/server/runtime";
import { activeStaff } from "@/server/staff-policy";
import Link from "next/link";
export default async function OvertimeDetail({params,searchParams}:{params:Promise<{holdingSlug:string;id:string}>;searchParams:Promise<{shift?:string}>}) {
 const {holdingSlug,id}=await params,actor=await requireWorkContext(holdingSlug),base=workspaceBase(publicScope(actor)),record=id==='new'?undefined:await getOvertime(actor,id).catch(e=>pageError(e,holdingSlug));
 let initial={};const {shift}=await searchParams;
 if(id==='new'&&shift){const {write}=await holdingResources(actor.holdingId),staff=await activeStaff(write,actor.companyId,actor.userId);const row=await write.prepare(`SELECT s.id,s.local_date,s.start_time,s.end_time,w.timezone,to_char(to_timestamp(s.ends_at/1000.0) AT TIME ZONE w.timezone,'YYYY-MM-DD"T"HH24:MI') end_local FROM people_roster_shifts s JOIN people_roster_weeks w ON w.id=s.week_id JOIN people_roster_assignments a ON a.shift_id=s.id WHERE s.id=? AND w.company_id=? AND w.state='published' AND s.state='active' AND a.engagement_id=? AND a.state='assigned'`).bind(shift,actor.companyId,staff.engagement_id).first<{id:string;local_date:string;start_time:string;end_local:string;timezone:string}>();if(row)initial={rosterShiftId:row.id,startLocal:`${row.local_date}T${row.start_time}`,endLocal:row.end_local,timezone:row.timezone};}
 return <FeaturePage className={`content ${styles.workspace}`}><Link href={`${base}/overtime`}>← My overtime</Link><header className={styles.heading}><div><p className={styles.eyebrow}>Personal request</p><h1>{record?'Overtime request':'Request overtime'}</h1><p>Enter details, review calculated hours, then submit.</p></div></header><FeatureTabs/><WidgetGrid pageKey="overtime-detail"><FeatureWidget id="overtime-request" title="Overtime request"><OvertimeForm key={`${actor.companyId}:${actor.userId}:${record?.id??'new'}`} base={base} record={record} initial={initial}/></FeatureWidget></WidgetGrid></FeaturePage>;
}
