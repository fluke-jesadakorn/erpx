import {SectionWorkspace} from "@/components/workplace/section-workspace";
import {RecordLink as Link} from "@/components/records/record-link";
import {ListFilters} from "@/components/ui/list-filters";
import {ProjectEditor} from "@/components/work_projection/staff-work-controls";
import {staffPresentationQueries} from "@/server/modules/work_projection/public/presentation-context-queries";
import {routeContextHref,type RouteSearch} from "@/lib/shared/route-context";
import {workspaceSearchContext} from "@/lib/work_projection/section-workspaces";
export default async function Projects({params,searchParams}:{params:Promise<{holdingSlug:string}>;searchParams:Promise<RouteSearch>}){
 const {holdingSlug}=await params,{base,options,projects,staff}=await staffPresentationQueries(holdingSlug),query=await searchParams,term=typeof query.q==='string'?query.q:'';
 const visible=projects.filter(project=>!term||`${project.title} ${project.description}`.toLocaleLowerCase().includes(term.toLocaleLowerCase()));
 return <SectionWorkspace title="Projects" selected="all" items={[{id:'all',title:'All projects',href:routeContextHref(`${base}/projects`,query,{view:null,detailKind:null,detailId:null})}]} context={`${visible.length} projects in your scope`}>
 <ListFilters appearance="standard" action={`${base}/projects`} query={term} preserve={workspaceSearchContext(query)} placeholder="Search projects"/>{staff&&<ProjectEditor appearance="standard" base={base} options={options}/>}
 <div className="ui-stack">{visible.map(project=><article className="ui-record-item" data-selected={query.detailId===project.id||undefined} key={project.id}><h2><Link href={`${base}/projects/${project.id}`}>{project.title}</Link></h2><p>{project.description}</p></article>)}{!visible.length&&<p>No projects match this view.</p>}</div>
 </SectionWorkspace>;
}
