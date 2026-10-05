import type {recordDomainData} from '@/server/record-domain-details';
import {BrandIdentityConsole} from '@/components/modules/brand-identity-console';
import {AgentGovernanceConsole} from '@/components/modules/agent-governance-console';
import {ProductionConsole} from '@/components/modules/production-console';
import {EcommerceConsole} from '@/components/modules/ecommerce-console';
import {LearningConsole} from '@/components/modules/learning-console';
import {BenefitsConsole} from '@/components/modules/benefits-console';
import {TalentConsole} from '@/components/modules/talent-console';
import {hasPermission} from '@/lib/permissions';
import type {WorkContext} from '@/server/authorization';
export function RecordDomainControls({detail,base,actor}:{detail:Awaited<ReturnType<typeof recordDomainData>>;base:string;actor:WorkContext}){
 if(detail.domain==='brand')return <BrandIdentityConsole base={base} data={detail.data} recordOnly/>;
 if(detail.domain==='agents'&&hasPermission(actor,'ai.manage'))return <AgentGovernanceConsole base={base} data={detail.data} recordOnly/>;
 if(detail.domain==='production'&&(hasPermission(actor,'records.write')||hasPermission(actor,'records.review')))return <ProductionConsole base={base} module={detail.module} data={detail.data} recordOnly/>;
 if(detail.domain==='ecommerce'&&(hasPermission(actor,'commerce.sales')||hasPermission(actor,'commerce.approve')))return <EcommerceConsole base={base} data={detail.data} recordOnly/>;
 if(detail.domain==='learning')return <LearningConsole base={base} data={detail.data} recordOnly/>;
 if(detail.domain==='benefits')return <BenefitsConsole base={base} data={detail.data} recordOnly/>;
 if(detail.domain==='talent')return <TalentConsole base={base} data={detail.data} recordOnly/>;
 return null;
}
