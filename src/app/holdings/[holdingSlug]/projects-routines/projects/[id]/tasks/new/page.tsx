import {ProjectPurchaseProvider} from '@/components/projects/purchase-context';
import {TaskEditor} from '@/components/work_projection/staff-work-controls';
import {RecordLink as Link} from '@/components/records/record-link';
import {FeaturePage} from '@/components/shared/features/feature-page';
import {InlineNotice} from '@/components/ui/foundation';
import {canonicalUiHref} from '@/lib/work_projection/destination-registry';
import {workspaceBase} from '@/lib/work_projection/workspace';
import {requireWorkContext,publicScope} from '@/server/modules/identity_access/public/authorization';
import {projectTaskCreationWorkspace} from '@/server/modules/projects/public/task-creation';
import {pageError} from '@/server/modules/work_projection/public/page-error';
export default async function NewProjectTask({params,searchParams}:{params:Promise<{holdingSlug:string;id:string}>;searchParams:Promise<Record<string,string|string[]|undefined>>}){
 const {holdingSlug,id}=await params,actor=await requireWorkContext(holdingSlug),base=workspaceBase(publicScope(actor)),data=await projectTaskCreationWorkspace(actor,id).catch(error=>pageError(error,holdingSlug)),query=await searchParams;
 return <ProjectPurchaseProvider purchases={data.purchases}><FeaturePage className="ui-page-content" data-source="live" data-module="projects"><Link href={canonicalUiHref(`${base}/projects/${encodeURIComponent(data.project.id)}/tasks`)}>← {data.project.title}</Link><h1>Create a Project task</h1><p>Assign work within {data.project.title}. The task keeps this Project as its source.</p><ProjectPurchaseProvider purchases={data.purchases} editorSource={{companyId:actor.companyId,actorId:actor.userId,available:data.canCreate,record:null}}>{data.canCreate&&<TaskEditor appearance="standard" base={base} options={data.options} projects={[data.project]} userId={actor.userId} companyId={actor.companyId} assignmentId={actor.assignmentId} fixedProjectId={data.project.id} initialDueDate={typeof query.dueDate==='string'?query.dueDate:undefined}/>}</ProjectPurchaseProvider>{!data.canCreate&&<InlineNotice tone="info">{data.reason}</InlineNotice>}</FeaturePage></ProjectPurchaseProvider>;
}
