import {handbookCreateHref} from '@erpx/feature-collaboration/lib/handbook-navigation';
import {learningConsoleSnapshot} from "@erpx/feature-learning/lib/purchase-controls";
import {handbookPresentationContext} from '@erpx/server-collaboration/public/handbook-purchases';
import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";
import {forbidden,notFound} from "next/navigation";
import {SectionWorkspace} from "@erpx/workplace/components/section-workspace";
import {RecordCatalog} from "@erpx/feature-records/components/record-catalog";
import {RecordLink as Link} from "@erpx/feature-records/components/record-link";
import {HandbookEditors} from "@erpx/workplace/components/handbook-controls";
import {LearningConsole} from "@erpx/feature-learning/components/learning-console";
import {ListFilters} from "@erpx/ui/components/list-filters";
import {ListPagination} from "@erpx/ui/components/list-pagination";
import {collaborationPresentationQueries} from "@erpx/server-collaboration/public/presentation-queries";
import {companyMembers} from "@erpx/server-organization/public/member-directory";
import {learningWorkspace} from "@erpx/server-learning/public/learning";
import {requireWorkContext,publicScope} from "@erpx/server-identity_access/public/authorization";
import {pageError} from "@erpx/server-work_projection/public/page-error";
import {workspaceBase} from "@erpx/feature-work_projection/lib/workspace";
import {hasModuleAccess} from "@erpx/catalogue/module-access";
import {hasPermission} from "@erpx/feature-identity_access/lib/permissions";
import {sectionWorkspaceTopics,knowledgeTopicsFor,workspaceTopicLinks,workspaceSearchContext} from "@erpx/feature-work_projection/lib/section-workspaces";
import type {RouteSearch} from "@erpx/feature-shared/lib/route-context";
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
 return <SectionWorkspace title="Knowledge" selected={selected} items={workspaceTopicLinks(base,query,topics)} context={topics.find(topic=>topic.id===selected)?.title} toolbar={view==='handbook'&&access.editor&&(!access.purchase||access.purchase.newWork)?<Link href={handbookCreateHref(base,section,query)}>New article</Link>:undefined}>
 {section!=='editors'&&<ListFilters appearance="standard" action={canonicalUiHref(`${base}/knowledge`)} query={term} placeholder="Search this view" preserve={workspaceSearchContext(query,{view,section})}/>}
 {articles&&<><div className="ui-stack">{articles.articles.map(article=><article className="ui-record-item" key={article.id}><small>{article.category.replaceAll('_',' ')}</small><h2><Link href={canonicalUiHref(`${base}/handbook/${article.id}${section==='drafts'?'?edit=1':''}`)}>{article.title||'Untitled draft'}</Link></h2><p>{article.excerpt}</p></article>)}{!articles.articles.length&&<p>No articles match this view.</p>}</div><ListPagination {...articles} query={{...query,view,section}} count={articles.articles.length}/></>}
 {view==='handbook'&&section==='editors'&&<HandbookEditors key={handbookPresentationContext(actor)} contextKey={handbookPresentationContext(actor)} purchase={access.purchase} base={base} companyId={actor.companyId} userId={actor.userId} options={await presentation.handbookEditorOptions(actor)} members={await companyMembers(actor)}/>}
 {data&&<>{section!=='mine'&&<LearningConsole embedded section={section==='courses'?'courses':'assignments'} base={base} data={learningConsoleSnapshot(data)}/>} {section==='courses'?<RecordCatalog base={base} kind="learning_course" title="Courses" rows={data.courses.filter(row=>!term||matching(row.title))} label="title"/>:<div className="ui-stack">{data.assignments.filter(row=>!term||matching(row.course_title)||matching(row.learner_name)).map(row=><article className="ui-record-item" key={String(row.id)}><Link href={canonicalUiHref(`${base}/learning/${row.id}`)}>{String(row.course_title)} · {String(row.learner_name)}</Link><p>{String(row.status)}</p></article>)}{!data.assignments.length&&<p>No learning assignments in this view.</p>}</div>}</>}
 </SectionWorkspace>;
}
