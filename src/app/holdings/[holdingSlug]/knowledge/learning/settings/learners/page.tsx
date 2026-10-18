import {LearningWorkerStartup} from '@/components/learning/worker-startup';
import {learningWorkerStartupWorkspace,learningWorkerStartupAccounts,learningWorkerSelectionSchema} from '@/server/modules/people_payroll/public/learning-worker-startup';
import {requireWorkContext,publicScope} from '@/server/modules/identity_access/public/authorization';
import {workspaceBase} from '@/lib/work_projection/workspace';
import {pageError} from '@/server/modules/work_projection/public/page-error';
export const dynamic='force-dynamic';
export default async function Page({params,searchParams}:{params:Promise<{holdingSlug:string}>;searchParams:Promise<Record<string,string|string[]|undefined>>}){
 const {holdingSlug}=await params,query=await searchParams,actor=await requireWorkContext(holdingSlug),selection=learningWorkerSelectionSchema.parse({...(query.accountId===undefined?{}:{accountId:query.accountId}),...(query.personId===undefined?{}:{personId:query.personId})});
 const initial=await learningWorkerStartupWorkspace(actor,selection).catch(error=>pageError(error,holdingSlug)),initialAccounts=initial.available?await learningWorkerStartupAccounts(actor,{}).catch(error=>pageError(error,holdingSlug)):{companyId:actor.companyId,accounts:[]};
 return <LearningWorkerStartup base={workspaceBase(publicScope(actor))} initial={initial} initialAccounts={initialAccounts}/>;
}
