import Link from "next/link";
import { z } from "zod";
import { requireWorkContext, publicScope } from "@/server/authorization";
import { workspaceBase } from "@/lib/workspace";
import { departmentOverrideDetail } from "@/server/department-decisions";
import { DepartmentOverrideDecision } from "@/components/modules/department-override-decision";
import { OperationSummary } from "@/components/workplace/operation-summary";

export const dynamic="force-dynamic";
export default async function DepartmentOverridePage({params,searchParams}:{params:Promise<{holdingSlug:string;kind:string;id:string}>;searchParams:Promise<{return?:string}>}){
  const route=await params,kind=z.enum(["expense","leave","overtime","request"]).parse(route.kind);
  const actor=await requireWorkContext(route.holdingSlug),base=workspaceBase(publicScope(actor));
  const detail=await departmentOverrideDetail(actor,kind,route.id);
  const returnQuery=new URLSearchParams((await searchParams).return??"");
  for(const key of [...returnQuery.keys()])if(!["view","q","queue","type","responsibility"].includes(key))returnQuery.delete(key);
  const returnTo=returnQuery.toString();
  const recordHref=`${base}/${kind==="request"?"requests":kind==="leave"?"tasks/people_payroll":kind==="overtime"?"overtime":"expenses"}/${encodeURIComponent(route.id)}${returnTo?`?return=${encodeURIComponent(returnTo)}`:""}`;
  return <main className="content-stack" data-source="live">
    <Link href={`${base}/work${returnTo?`?${returnTo}`:""}`}>← Work</Link>
    <h1>Decision on behalf of a department approver</h1>
    <OperationSummary operation={{status:"pending",responsible:detail.originalApprover,currentStep:`Department approval · step ${detail.step}`,nextAction:"Record your decision and reason",blockedReason:null,dueAt:null,recordHref,actionKind:"override"}}/>
    <p className="muted">Your current reporting position is above the assigned approver. Record why you are making this department decision. Later specialist steps continue in order.</p>
    {kind==="leave"&&<p className="muted">Leave details and attachments are limited to the requester and assigned reviewers.</p>}
    <DepartmentOverrideDecision base={base} detail={detail} returnTo={returnTo}/>
    {!!detail.priorOverrides.length&&<section className="card"><h2>Previous decisions on behalf of an approver</h2>{detail.priorOverrides.map((event,index)=><p key={index}>{event.actor} decided for {event.originalApprover} · {new Date(event.at).toLocaleString("en-GB",{timeZone:"Asia/Bangkok"})}{event.reason?` · ${event.reason}`:""}</p>)}</section>}
  </main>;
}
