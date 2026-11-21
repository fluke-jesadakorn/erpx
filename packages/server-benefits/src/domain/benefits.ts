import type { WorkItem } from "@erpx/feature-work_projection/lib/work-item";

export type BenefitRow = Record<string, string | number | null>;
export type BenefitRecordKind = "plan" | "window" | "dependent" | "election";
export function benefitWorkItems(rows:BenefitRow[],person:BenefitRow|null,actorId:string,canReview:boolean):WorkItem[]{
 return rows.map(x=>{
  const review=["submitted","acknowledged"].includes(String(x.status));
  const canAct=review?canReview&&(x.status!=="acknowledged"||x.acknowledged_by!==actorId):x.person_id===person?.id;
  return {id:String(x.id),type:"benefit_enrollment",title:`Benefits · ${x.plan_name}`,detail:`${x.employee_name} · ${x.status}`,href:`/benefits/${x.id}`,status:String(x.status),updatedAt:Number(x.updated_at),queue:canAct?"todo":"waiting",canAct,nextAction:String(x.status)==="elected"?"Submit election":String(x.status)==="submitted"?"Record provider acknowledgement":"Release payroll input",relationships:[x.person_id===person?.id?"requested":"team",...(review?["approval" as const]:[])]};
 });
}
