import {forbidden,notFound} from "next/navigation";
import {SectionWorkspace} from "@/components/workplace/section-workspace";
import {RecordCatalog} from "@/components/records/record-catalog";
import {RecordLink as Link} from "@/components/records/record-link";
import {HandbookEditors} from "@/components/workplace/handbook-controls";
import {LearningConsole} from "@/components/learning/learning-console";
import {ListFilters} from "@/components/ui/list-filters";
import {ListPagination} from "@/components/ui/list-pagination";
import {collaborationPresentationQueries} from "@/server/modules/collaboration/public/presentation-queries";
import {companyMembers} from "@/server/modules/organization/public/member-directory";
import {learningWorkspace} from "@/server/modules/learning/public/learning";
import {requireWorkContext,publicScope} from "@/server/modules/identity_access/public/authorization";
import {pageError} from "@/server/modules/work_projection/public/page-error";
import {workspaceBase} from "@/lib/work_projection/workspace";
import {hasModuleAccess} from "@/data/module-access";
import {hasPermission} from "@/lib/identity_access/permissions";
import {sectionWorkspaceTopics,knowledgeTopicsFor,workspaceTopicLinks,workspaceSearchContext} from "@/lib/work_projection/section-workspaces";
import type {RouteSearch} from "@/lib/shared/route-context";
export const dynamic='force-dynamic';
export default async function Knowledge({params,searchParams}:{params:Promise<{holdingSlug:string}>;searchParams:Promise<RouteSearch>}){
 const {holdingSlug}=await params,actor=await requireWorkContext(holdingSlug),base=workspaceBase(publicScope(actor)),query=await searchParams,presentation=await collaborationPresentationQueries(actor.holdingId);
 const access=await presentation.handbookAccess(actor).catch(error=>pageError(error,holdingSlug));
 const learning=hasModuleAccess(actor.modules,'learning')&&hasPermission(actor,'records.read');
 const topics=knowledgeTopicsFor(access,learning);
 const view=typeof query.view==='string'?query.view:'handbook',section=typeof query.section==='string'?query.section:view==='learning'?'courses':'published',selected=`${view}:${section}`;
 if(!sectionWorkspaceTopics.knowledge.some(topic=>topic.id===selected))notFound();if(!topics.some(topic=>topic.id===selected))forbidden();
 const term=typeof query.q==='string'?query.q:'',cursor=typeof query.cursor==='string'?query.cursor:undefined;
 const articles=view==='handbook'&&section!=='editors'?await presentation.handbookList(actor,term,section==='drafts',cursor).catch(error=>pageError(error,holdingSlug)):null;
 const data=view==='learning'?await learningWorkspace(actor,section==='mine').catch(error=>pageError(error,holdingSlug)):null;
 const matching=(value:unknown)=>String(value??'').toLocaleLowerCase().includes(term.toLocaleLowerCase());
 return <SectionWorkspace title="Knowledge" selected={selected} items={workspaceTopicLinks(base,query,topics)} context={topics.find(topic=>topic.id===selected)?.title} toolbar={view==='handbook'&&access.editor?<Link href={`${base}/handbook/new`}>New article</Link>:undefined}>
 {section!=='editors'&&<ListFilters appearance="standard" action={`${base}/knowledge`} query={term} placeholder="Search this view" preserve={workspaceSearchContext(query,{view,section})}/>}
 {articles&&<><div className="ui-stack">{articles.articles.map(article=><article className="ui-record-item" key={article.id}><small>{article.category.replaceAll('_',' ')}</small><h2><Link href={`${base}/handbook/${article.id}${section==='drafts'?'?edit=1':''}`}>{article.title||'Untitled draft'}</Link></h2><p>{article.excerpt}</p></article>)}{!articles.articles.length&&<p>No articles match this view.</p>}</div><ListPagination {...articles} query={{...query,view,section}} count={articles.articles.length}/></>}
 {view==='handbook'&&section==='editors'&&<HandbookEditors base={base} companyId={actor.companyId} userId={actor.userId} options={await presentation.handbookEditorOptions(actor)} members={await companyMembers(actor)}/>}
 {data&&<>{section!=='mine'&&<LearningConsole embedded section={section==='courses'?'courses':'assignments'} base={base} data={data}/>} {section==='courses'?<RecordCatalog base={base} kind="learning_course" title="Courses" rows={data.courses.filter(row=>!term||matching(row.title))} label="title"/>:<div className="ui-stack">{data.assignments.filter(row=>!term||matching(row.course_title)||matching(row.learner_name)).map(row=><article className="ui-record-item" key={String(row.id)}><Link href={`${base}/learning/${row.id}`}>{String(row.course_title)} · {String(row.learner_name)}</Link><p>{String(row.status)}</p></article>)}{!data.assignments.length&&<p>No learning assignments in this view.</p>}</div>}</>}
 </SectionWorkspace>;
}
