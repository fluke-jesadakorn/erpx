import WorkspaceLink from "@/components/workplace/workspace-link";
import {requireWorkContext} from "@/server/modules/identity_access/public/authorization";
import {hasPermission} from "@/lib/identity_access/permissions";
import {projectWorkerStartupWorkspace} from "@/server/modules/people_payroll/public/project-worker-startup";
import {ProjectPurchaseProvider} from "@/components/projects/purchase-context";
import {projectPurchased} from "@/lib/projects/purchase-controls";
import {projectMemberPickerOptions} from "@/lib/projects/worker-display";
import { canonicalUiHref } from "@/lib/work_projection/destination-registry";
import {SectionWorkspace} from "@/components/workplace/section-workspace";
import {RecordLink as Link} from "@/components/records/record-link";
import {ListFilters} from "@/components/ui/list-filters";
import {ProjectEditor} from "@/components/work_projection/staff-work-controls";
import {staffPresentationQueries} from "@/server/modules/work_projection/public/presentation-context-queries";
import {routeContextHref,type RouteSearch} from "@/lib/shared/route-context";
import {workspaceSearchContext} from "@/lib/work_projection/section-workspaces";
export default async function Projects({params,searchParams}:{params:Promise<{holdingSlug:string}>;searchParams:Promise<RouteSearch>}){
 const {holdingSlug}=await params,{actor,base,options,projects,staff,queries}=await staffPresentationQueries(holdingSlug),query=await searchParams,term=typeof query.q==='string'?query.q:'';
 const purchases=await queries.projectPurchases();
 let canPrepareWorker=false;
 if(!staff){
  const current=await requireWorkContext(holdingSlug);
  if(current.companyId===actor.companyId&&current.holdingId===actor.holdingId&&!current.beta&&current.scope.kind==='holding'&&!current.branchId&&!current.departmentId&&hasPermission(current,'people.manage')&&hasPermission(current,'organization.manage')){
   const preparation=await projectWorkerStartupWorkspace(current,{});
   canPrepareWorker=preparation.available&&preparation.companyId===current.companyId;
  }
 }
 const visible=projects.filter(project=>!term||`${project.title} ${project.description}`.toLocaleLowerCase().includes(term.toLocaleLowerCase()));
 return <ProjectPurchaseProvider purchases={purchases}><SectionWorkspace title="Projects" selected="all" items={[{id:'all',title:'All projects',href:routeContextHref(`${base}/projects`,query,{view:null,detailKind:null,detailId:null})}]} context={`${visible.length} projects in your scope`}>
 <ListFilters appearance="standard" action={canonicalUiHref(`${base}/projects`)} query={term} preserve={workspaceSearchContext(query)} placeholder="Search projects"/><ProjectPurchaseProvider purchases={purchases} editorSource={{companyId:actor.companyId,actorId:actor.userId,available:Boolean(staff),record:null}}>{staff&&projectPurchased(purchases,"projects.wbs","newWork")&&<ProjectEditor appearance="standard" base={base} commandOwner={{companyId:actor.companyId,actorId:actor.userId}} options={projectMemberPickerOptions(options)}/>}</ProjectPurchaseProvider>
 {canPrepareWorker&&<p>Your Company account needs an active worker profile before it can create or join Project work. <WorkspaceLink href={`${base}/projects-routines/projects/settings/workers`}>Prepare a Project worker</WorkspaceLink></p>}
 <div className="ui-stack">{visible.map(project=><article className="ui-record-item" data-selected={query.detailId===project.id||undefined} key={project.id}><h2><Link href={canonicalUiHref(`${base}/projects/${project.id}`)}>{project.title}</Link></h2><p>{project.description}</p></article>)}{!visible.length&&<p>No projects match this view.</p>}</div>
 </SectionWorkspace></ProjectPurchaseProvider>;
}
