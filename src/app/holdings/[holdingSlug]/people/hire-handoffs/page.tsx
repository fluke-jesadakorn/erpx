import { RecordLink as Link } from "@/components/records/record-link";
import {FeaturePage} from "@/components/features/feature-page";
import {HireHandoffConsole} from "@/components/modules/hire-handoff-console";
import {workspaceBase} from "@/lib/workspace";
import {publicScope,requireWorkContext} from "@/server/authorization";
import {hireHandoffQueue} from "@/server/people-handoffs";
import {holdingResources} from "@/server/runtime";
import {pageError} from "@/server/page-error";
export const dynamic="force-dynamic";
export default async function HireHandoffsPage({params}:{params:Promise<{holdingSlug:string}>}){
 const {holdingSlug}=await params,actor=await requireWorkContext(holdingSlug),base=workspaceBase(publicScope(actor));let handoffs;try{handoffs=await hireHandoffQueue(actor);}catch(error){return pageError(error,holdingSlug);}const {write}=await holdingResources(actor.holdingId),[groups,people]=await Promise.all([write.prepare("SELECT id,name,currency,frequency FROM hr_pay_groups WHERE company_id=? AND status='active' ORDER BY name,id").bind(actor.companyId).all(),write.prepare("SELECT id,legal_name,email,version FROM hr_people WHERE company_id=? AND status='active' AND merged_into_id IS NULL ORDER BY legal_name,id").bind(actor.companyId).all()]);
 return <FeaturePage className="ui-page-content" data-source="live"><Link href={`${base}/people`}>← People</Link><h1>Accepted offers and HR onboarding</h1><p>Review the current offer and candidate identity, choose a Company pay group, and reconcile an existing person when appropriate. The original Talent handoff and its HR consumption receipt remain immutable.</p><HireHandoffConsole base={base} handoffs={handoffs} groups={groups.results} people={people.results}/></FeaturePage>;
}
