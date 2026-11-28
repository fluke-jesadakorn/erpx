import {handbookCreateHref} from '@erpx/collaboration-ui/lib/handbook-navigation';
import {learningConsoleSnapshot} from "@erpx/learning-ui/lib/purchase-controls";
import {handbookPresentationContext} from '@erpx/collaboration-server/public/handbook-purchases';
import { canonicalUiHref } from "@erpx/work-projection-ui/lib/destination-registry";
import {forbidden,notFound} from "next/navigation";
import {SectionWorkspace} from "@erpx/platform-workplace/components/section-workspace";
import {RecordCatalog} from "@erpx/records-ui/components/record-catalog";
import {RecordLink as Link} from "@erpx/records-ui/components/record-link";
import {HandbookEditors} from "@erpx/platform-workplace/components/handbook-controls";
import {LearningConsole} from "@erpx/learning-ui/components/learning-console";
import {ListFilters} from "@erpx/platform-ui/components/list-filters";
import {ListPagination} from "@erpx/platform-ui/components/list-pagination";
import { handbookReads } from "@erpx/collaboration-ui/lib/native-handbook-page";
import {companyMembers} from "@erpx/organization-server/public/member-directory";
import {nativeLearningWorkspacePage} from "@erpx/platform-workplace/lib/native-learning";
import {requireWorkContext,publicScope} from "@erpx/identity-access-server/public/authorization";
import {pageError} from "@erpx/work-projection-server/public/page-error";
import {workspaceBase} from "@erpx/work-projection-ui/lib/workspace";
import {hasModuleAccess} from "@erpx/platform-catalogue/module-access";
import {hasPermission} from "@erpx/identity-access-ui/lib/permissions";
import {sectionWorkspaceTopics,knowledgeTopicsFor,workspaceTopicLinks,workspaceSearchContext} from "@erpx/work-projection-ui/lib/section-workspaces";
import type {RouteSearch} from "@erpx/platform-shared-ui/lib/route-context";
export const dynamic='force-dynamic';
export default async function Knowledge({params,searchParams}:{params:Promise<{holdingSlug:string}>;searchParams:Promise<RouteSearch>}){
 const {holdingSlug}=await params,actor=await requireWorkContext(holdingSlug),base=workspaceBase(publicScope(actor)),query=await searchParams,presentation=await handbookReads(actor,holdingSlug);
 const access=await presentation.handbookAccess(actor).catch(error=>pageError(error,holdingSlug));
 const learning=hasModuleAccess(actor.modules,'learning')&&hasPermission(actor,'records.read');
 const topics=knowledgeTopicsFor(access,learning);
 const view=typeof query.view==='string'?query.view:'handbook',section=typeof query.section==='string'?query.section:view==='learning'?'courses':'published',selected=`${view}:${section}`;
 if(!sectionWorkspaceTopics.knowledge.some(topic=>topic.id===selected))notFound();if(!topics.some(topic=>topic.id===selected))forbidden();
 const term=typeof query.q==='string'?query.q:'',cursor=typeof query.cursor==='string'?query.cursor:undefined;
 const articles=view==='handbook'&&section!=='editors'?await presentation.handbookList(actor,term,section==='drafts',cursor).catch(error=>pageError(error,holdingSlug)):null;
 const data=view==='learning'?await nativeLearningWorkspacePage(actor,section==='mine').catch(error=>pageError(error,holdingSlug)):null;
 const matching=(value:unknown)=>String(value??'').toLocaleLowerCase().includes(term.toLocaleLowerCase());
 return <SectionWorkspace title="Knowledge" selected={selected} items={workspaceTopicLinks(base,query,topics)} context={topics.find(topic=>topic.id===selected)?.title} toolbar={view==='handbook'&&access.editor&&(!access.purchase||access.purchase.newWork)?<Link href={handbookCreateHref(base,section,query)}>New article</Link>:undefined}>
 {section!=='editors'&&<ListFilters appearance="standard" action={canonicalUiHref(`${base}/knowledge`)} query={term} placeholder="Search this view" preserve={workspaceSearchContext(query,{view,section})}/>}
 {articles&&<><div className="ui-stack">{articles.articles.map(article=><article className="ui-record-item" key={article.id}><small>{article.category.replaceAll('_',' ')}</small><h2><Link href={canonicalUiHref(`${base}/handbook/${article.id}${section==='drafts'?'?edit=1':''}`)}>{article.title||'Untitled draft'}</Link></h2><p>{article.excerpt}</p></article>)}{!articles.articles.length&&<p>No articles match this view.</p>}</div><ListPagination {...articles} query={{...query,view,section}} count={articles.articles.length}/></>}
 {view==='handbook'&&section==='editors'&&<HandbookEditors key={handbookPresentationContext(actor)} contextKey={handbookPresentationContext(actor)} purchase={access.purchase} base={base} companyId={actor.companyId} userId={actor.userId} options={await presentation.handbookEditorOptions(actor)} members={await companyMembers(actor)}/>}
 {data&&<>{section!=='mine'&&<LearningConsole embedded section={section==='courses'?'courses':'assignments'} base={base} data={learningConsoleSnapshot(data as unknown as Parameters<typeof learningConsoleSnapshot>[0])}/>} {section==='courses'?<RecordCatalog base={base} kind="learning_course" title="Courses" rows={data.courses.filter(row=>!term||matching(row.title))} label="title"/>:<div className="ui-stack">{data.assignments.filter(row=>!term||matching(row.course_title)||matching(row.learner_name)).map(row=><article className="ui-record-item" key={String(row.id)}><Link href={canonicalUiHref(`${base}/learning/${row.id}`)}>{String(row.course_title)} · {String(row.learner_name)}</Link><p>{String(row.status)}</p></article>)}{!data.assignments.length&&<p>No learning assignments in this view.</p>}</div>}</>}
 </SectionWorkspace>;
}
