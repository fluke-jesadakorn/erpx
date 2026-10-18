import {PayrollPostingPrerequisites} from '@/components/records/loaders/modules--payroll-posting-prerequisites';
import {PageContent} from '@/components/ui/foundation';
import {hasPermission} from '@/lib/identity_access/permissions';
import {workspaceBase} from '@/lib/work_projection/workspace';
import {publicScope,requireWorkContext} from '@/server/modules/identity_access/public/authorization';
import {payrollPostingPrerequisites} from '@/server/modules/finance/public/payroll-posting-prerequisites';
import {pageError} from '@/server/modules/work_projection/public/page-error';
import {forbidden} from 'next/navigation';
export const dynamic='force-dynamic';
export default async function PayrollPostingSettings({params}:{params:Promise<{holdingSlug:string}>}){
 const {holdingSlug}=await params,actor=await requireWorkContext(holdingSlug);
 if(actor.branchId||actor.departmentId||!hasPermission(actor,'finance.approve'))forbidden();
 let initial:Awaited<ReturnType<typeof payrollPostingPrerequisites>>;
 try{initial=await payrollPostingPrerequisites(actor);}catch(error){return pageError(error,actor.holdingSlug);}
 return <PageContent as="div" className="ui-page-content" data-source="live"><PayrollPostingPrerequisites base={workspaceBase(publicScope(actor))} initial={initial}/></PageContent>;
}
