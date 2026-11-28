import {learningConsoleSnapshot} from "@erpx/learning-ui/lib/purchase-controls";
import type {recordDomainData} from "@erpx/records-server/public/record-domain-details";
import { BrandIdentityConsole } from "@erpx/records-ui/components/loaders/modules--brand-identity-console";
import { AgentGovernanceConsole } from "@erpx/records-ui/components/loaders/modules--agent-governance-console";
import { ProductionConsole } from "@erpx/records-ui/components/loaders/modules--production-console";
import { EcommerceConsole } from "@erpx/records-ui/components/loaders/modules--ecommerce-console";
import { LearningConsole } from "@erpx/records-ui/components/loaders/modules--learning-console";
import { BenefitsConsole } from "@erpx/records-ui/components/loaders/modules--benefits-console";
import { TalentConsole } from "@erpx/records-ui/components/loaders/modules--talent-console";
import {hasPermission} from "@erpx/identity-access-ui/lib/permissions";
import type {WorkContext} from "@erpx/identity-access-server/public/authorization";
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
