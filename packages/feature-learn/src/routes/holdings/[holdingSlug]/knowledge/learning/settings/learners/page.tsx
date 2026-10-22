import {LearningWorkerStartup} from '@erpx/feature-learning/components/worker-startup';
import {learningWorkerStartupWorkspace,learningWorkerStartupAccounts,learningWorkerSelectionSchema} from '@erpx/server-people_payroll/public/learning-worker-startup';
import {requireWorkContext,publicScope} from '@erpx/server-identity_access/public/authorization';
import {workspaceBase} from '@erpx/feature-work_projection/lib/workspace';
import {pageError} from '@erpx/server-work_projection/public/page-error';
export const dynamic='force-dynamic';
export default async function Page({params,searchParams}:{params:Promise<{holdingSlug:string}>;searchParams:Promise<Record<string,string|string[]|undefined>>}){
 const {holdingSlug}=await params,query=await searchParams,actor=await requireWorkContext(holdingSlug),selection=learningWorkerSelectionSchema.parse({...(query.accountId===undefined?{}:{accountId:query.accountId}),...(query.personId===undefined?{}:{personId:query.personId})});
 const initial=await learningWorkerStartupWorkspace(actor,selection).catch(error=>pageError(error,holdingSlug)),initialAccounts=initial.available?await learningWorkerStartupAccounts(actor,{}).catch(error=>pageError(error,holdingSlug)):{companyId:actor.companyId,accounts:[]};
 return <LearningWorkerStartup base={workspaceBase(publicScope(actor))} initial={initial} initialAccounts={initialAccounts}/>;
}
