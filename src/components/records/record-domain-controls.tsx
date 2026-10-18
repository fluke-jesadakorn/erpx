import {learningConsoleSnapshot} from "@/lib/learning/purchase-controls";
import type {recordDomainData} from "@/server/modules/records/public/record-domain-details";
import { BrandIdentityConsole } from "@/components/records/loaders/modules--brand-identity-console";
import { AgentGovernanceConsole } from "@/components/records/loaders/modules--agent-governance-console";
import { ProductionConsole } from "@/components/records/loaders/modules--production-console";
import { EcommerceConsole } from "@/components/records/loaders/modules--ecommerce-console";
import { LearningConsole } from "@/components/records/loaders/modules--learning-console";
import { BenefitsConsole } from "@/components/records/loaders/modules--benefits-console";
import { TalentConsole } from "@/components/records/loaders/modules--talent-console";
import {hasPermission} from "@/lib/identity_access/permissions";
import type {WorkContext} from "@/server/modules/identity_access/public/authorization";
export function RecordDomainControls({detail,base,actor}:{detail:Awaited<ReturnType<typeof recordDomainData>>;base:string;actor:WorkContext}){
 if(detail.domain==='brand')return <BrandIdentityConsole base={base} data={detail.data} recordOnly/>;
 if(detail.domain==='agents'&&hasPermission(actor,'ai.manage'))return <AgentGovernanceConsole base={base} data={detail.data} recordOnly/>;
 if(detail.domain==='production'&&(hasPermission(actor,'records.write')||hasPermission(actor,'records.review')))return <ProductionConsole base={base} module={detail.module} data={detail.data} reviewActor={{userId:actor.userId,companyId:actor.companyId,assignmentId:actor.assignmentId,branchId:actor.branchId,departmentId:actor.departmentId}} recordOnly/>;
 if(detail.domain==='ecommerce'&&(hasPermission(actor,'commerce.sales')||hasPermission(actor,'commerce.approve')))return <EcommerceConsole base={base} data={detail.data} recordOnly/>;
 if(detail.domain==='learning')return <LearningConsole base={base} data={learningConsoleSnapshot({...detail.data,people:[]})} recordOnly/>;
 if(detail.domain==='benefits')return <BenefitsConsole base={base} data={detail.data} recordOnly/>;
 if(detail.domain==='talent')return <TalentConsole base={base} data={detail.data} recordOnly/>;
 return null;
}
