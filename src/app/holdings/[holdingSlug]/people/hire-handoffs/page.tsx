import { peoplePayrollPresentationQueries } from "@/server/modules/people_payroll/public/presentation-queries";

import { RecordLink as Link } from "@/components/records/record-link";
import {FeaturePage} from "@/components/shared/features/feature-page";
import {HireHandoffConsole} from "@/components/people_payroll/hire-handoff-console";
import {workspaceBase} from "@/lib/work_projection/workspace";
import {publicScope,requireWorkContext} from "@/server/modules/identity_access/public/authorization";
import {hireHandoffQueue} from "@/server/modules/people_payroll/public/people-handoffs";

import {pageError} from "@/server/modules/work_projection/public/page-error";
export const dynamic="force-dynamic";
export default async function HireHandoffsPage({params}:{params:Promise<{holdingSlug:string}>}){
 const {holdingSlug}=await params,actor=await requireWorkContext(holdingSlug),base=workspaceBase(publicScope(actor));let handoffs;try{handoffs=await hireHandoffQueue(actor);}catch(error){return pageError(error,holdingSlug);}const presentation=await peoplePayrollPresentationQueries(actor.holdingId),[groups,people]=await Promise.all([presentation.hireHandoffPayGroups(actor.companyId),presentation.hireHandoffPeople(actor.companyId)]);
 return <FeaturePage className="ui-page-content" data-source="live"><Link href={`${base}/people`}>← People</Link><h1>Accepted offers and HR onboarding</h1><p>Review the current offer and candidate identity, choose a Company pay group, and reconcile an existing person when appropriate. The original Talent handoff and its HR consumption receipt remain immutable.</p><HireHandoffConsole base={base} handoffs={handoffs} groups={groups.results} people={people.results}/></FeaturePage>;
}
