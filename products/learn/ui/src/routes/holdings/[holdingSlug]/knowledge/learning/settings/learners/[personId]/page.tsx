import {LearningWorkerStartup} from '@erpx/learning-ui/components/worker-startup';
import {learningWorkerStartupWorkspace,learningWorkerStartupAccounts,learningWorkerSelectionSchema} from '@erpx/people-payroll-server/public/learning-worker-startup';
import {requireWorkContext,publicScope} from '@erpx/identity-access-server/public/authorization';
import {workspaceBase} from '@erpx/work-projection-ui/lib/workspace';
import {pageError} from '@erpx/work-projection-server/public/page-error';
export const dynamic='force-dynamic';
export default async function Page({params}:{params:Promise<{holdingSlug:string;personId:string}>}){
 const {holdingSlug,personId}=await params,actor=await requireWorkContext(holdingSlug),selection=learningWorkerSelectionSchema.parse({personId});
 const initial=await learningWorkerStartupWorkspace(actor,selection).catch(error=>pageError(error,holdingSlug)),initialAccounts=initial.available?await learningWorkerStartupAccounts(actor,{}).catch(error=>pageError(error,holdingSlug)):{companyId:actor.companyId,accounts:[]};
 return <LearningWorkerStartup base={workspaceBase(publicScope(actor))} initial={initial} initialAccounts={initialAccounts}/>;
}
