import {PayrollPostingPrerequisites} from '@erpx/feature-records/components/loaders/modules--payroll-posting-prerequisites';
import {PageContent} from '@erpx/ui/components/foundation';
import {hasPermission} from '@erpx/feature-identity_access/lib/permissions';
import {workspaceBase} from '@erpx/feature-work_projection/lib/workspace';
import {publicScope,requireWorkContext} from '@erpx/server-identity_access/public/authorization';
import {payrollPostingPrerequisites} from '@erpx/server-finance/public/payroll-posting-prerequisites';
import {pageError} from '@erpx/server-work_projection/public/page-error';
import {forbidden} from 'next/navigation';
export const dynamic='force-dynamic';
export default async function PayrollPostingSettings({params}:{params:Promise<{holdingSlug:string}>}){
 const {holdingSlug}=await params,actor=await requireWorkContext(holdingSlug);
 if(actor.branchId||actor.departmentId||!hasPermission(actor,'finance.approve'))forbidden();
 let initial:Awaited<ReturnType<typeof payrollPostingPrerequisites>>;
 try{initial=await payrollPostingPrerequisites(actor);}catch(error){return pageError(error,actor.holdingSlug);}
 return <PageContent as="div" className="ui-page-content" data-source="live"><PayrollPostingPrerequisites base={workspaceBase(publicScope(actor))} initial={initial}/></PageContent>;
}
