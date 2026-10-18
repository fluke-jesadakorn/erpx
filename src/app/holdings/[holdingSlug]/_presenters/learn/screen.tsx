import { canonicalUiHref } from "@/lib/work_projection/destination-registry";
import { collaborationPresentationQueries } from "@/server/modules/collaboration/public/presentation-queries";

import { PageContent } from "@/components/ui/patterns";

import { ActionLink as UiActionLink } from "@/components/ui/foundation";
import { FeaturePage, FeatureWidget,WidgetGrid } from "@/components/shared/features/feature-page";
import { ListFilters } from "@/components/ui/list-filters";
import { ListPagination } from "@/components/ui/list-pagination";
import { EmptyState,PageHeader } from "@/components/ui/primitives";
import { pageGuides } from "@/lib/work_projection/ai/workspace-assistant";
import { workplaceNavigation } from "@/lib/work_projection/workplace-navigation";
import { navigationContext } from "@/server/modules/work_projection/public/navigation-context";
import { workspaceBase } from "@/lib/work_projection/workspace";
import { publicScope,requireWorkContext } from "@/server/modules/identity_access/public/authorization";


import { ArrowUpRight,BookOpen,Sparkles } from "lucide-react";
import { RecordLink as Link } from "@/components/records/record-link";
import styles from "@/app/holdings/[holdingSlug]/_presenters/learn/learn.module.css";

export default async function LearnPage({params,searchParams}:{params:Promise<{holdingSlug:string}>;searchParams:Promise<{q?:string;cursor?:string}>}) {
  const actor=await requireWorkContext((await params).holdingSlug),base=workspaceBase(publicScope(actor)),query=await searchParams;
  const presentation=await collaborationPresentationQueries(actor.holdingId);
  const knowledge=await presentation.handbookList(actor, query.q??"", false, query.cursor);
  const destinations=new Map<string,string>();
  for(const item of workplaceNavigation(await navigationContext(actor)).search) {
    const path=item.href.slice(base.length).split(/[?#]/)[0].split("/").filter(Boolean)[0];
    if(path && path!=="learn" && pageGuides[path] && !destinations.has(path))destinations.set(path,item.href);
  }
  const guides=Object.entries(pageGuides).filter(([path,guide])=>destinations.has(path)&&(!query.q||`${guide.title} ${guide.purpose} ${guide.prompts.join(" ")}`.toLowerCase().includes(query.q.toLowerCase())));
  return <PageContent as={FeaturePage} data-source="live">
    <PageHeader appearance="standard" eyebrow="Learn in the flow of work" title="Guides" description="Practical workflow guides and published company knowledge, close to the work you need to do." action={knowledge.access.editor?<UiActionLink appearance="standard" variant="secondary" href={canonicalUiHref(`${base}/handbook?edit=1`)}>Manage company knowledge</UiActionLink>:undefined}/><WidgetGrid pageKey="learn">
    <FeatureWidget appearance="standard" id="learn-open-the-work-ask-what-comes-next-" title="Open the work. Ask what comes next."><div className={styles.symbol}><Sparkles size={27}/></div><div><span className={styles.eyebrow}>YOUR ON-PAGE COACH</span><h2>Open the work. Ask what comes next.</h2><p>Choose a workflow below, then open <strong>Ask ERPX → Explain</strong> for its steps. Ask in any language and receive clear English guidance. AI uses the current page’s supported sources and your selected role.</p></div><UiActionLink appearance="standard" variant="primary" href={canonicalUiHref(`${base}/work`)}>Open my work <ArrowUpRight size={16}/></UiActionLink></FeatureWidget>
    <ListFilters appearance="standard" action={canonicalUiHref(`${base}/learn`)} query={query.q} placeholder="Search workflows and company knowledge"/>
    <FeatureWidget appearance="standard" id="learn-captioned-lessons" title="Captioned lessons">
      <h2>Captioned lessons</h2>
      <h3>A welcoming conversation</h3>
      <p>Practice a friendly welcome, listen carefully, and explain the next step.</p>
      <video controls playsInline preload="none" aria-label="A welcoming conversation" style={{ width: "100%", height: "auto", aspectRatio: "16 / 9" }}>
        <source src="/preview/lessons/welcome.mp4" type="video/mp4" />
        <track src="/preview/lessons/welcome.vtt" kind="captions" srcLang="en" label="English" default />
        Your browser does not support video playback. <a href="/preview/lessons/welcome.vtt">Read the English captions</a>.
      </video>
    </FeatureWidget>
    <FeatureWidget appearance="standard" id="learn-choose-your-workflow" title="Choose your workflow"><div className={styles.sectionHeading}><div><span className={styles.eyebrow}>GET SOMETHING DONE</span><h2 id="workflow-heading">Choose your workflow</h2></div><span className={styles.muted}>{guides.length} guides · Available without AI</span></div>
      {!guides.length&&<EmptyState title="No matching workflows" description="Try another topic or search your company handbook below."/>}
      <div className={styles.grid}>{guides.map(([path,guide])=><article key={path} className={styles.guide}><div className={styles.cardTop}><BookOpen size={20}/><span>{guide.steps.length} steps</span></div><h3>{guide.title}</h3><p>{guide.purpose}</p><ol>{guide.steps.map(step=><li key={step}>{step}</li>)}</ol><Link href={destinations.get(path)!}>Open {guide.title.toLowerCase()} <ArrowUpRight size={16}/></Link></article>)}</div>
    </FeatureWidget>
    <FeatureWidget appearance="standard" id="learn-published-knowledge" title="Published knowledge"><div className={styles.sectionHeading}><div><span className={styles.eyebrow}>YOUR COMPANY’S WAY OF WORKING</span><h2 id="knowledge-heading">Published knowledge</h2></div><Link href={canonicalUiHref(`${base}/handbook`)}>Browse handbook →</Link></div><p className={styles.muted}>Policies, SOPs, FAQs and service guides published by your company. Open an article and ask ERPX to explain that revision.</p>
      {!knowledge.articles.length?<EmptyState title="No published knowledge matches" description={knowledge.access.editor?"Create and publish an article in Company handbook to make reviewed guidance available here.":"Published guidance will appear here when a company editor makes it available."}/>:<div className={styles.knowledge}>{knowledge.articles.map(article=><Link key={article.id} href={canonicalUiHref(`${base}/handbook/${article.id}`)}><span className={styles.eyebrow}>{article.category.replaceAll("_"," ")} · Revision {article.revision}</span><h3>{article.title}</h3><p>{article.excerpt}</p><span>Read & ask AI →</span></Link>)}</div>}
      <ListPagination {...knowledge} query={query} count={knowledge.articles.length}/>
    </FeatureWidget>
  </WidgetGrid></PageContent>;
}
