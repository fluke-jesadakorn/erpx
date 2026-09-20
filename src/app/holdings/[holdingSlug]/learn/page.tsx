import { FeaturePage, FeatureWidget,WidgetGrid } from "@/components/features/feature-page";
import { PeopleNavigation } from "@/components/modules/people-navigation";
import { ListFilters } from "@/components/ui/list-filters";
import { ListPagination } from "@/components/ui/list-pagination";
import { EmptyState,PageHeader } from "@/components/ui/primitives";
import { pageGuides } from "@/lib/ai/workspace-assistant";
import { hasPermission } from "@/lib/permissions";
import { workspaceBase } from "@/lib/workspace";
import { publicScope,requireWorkContext } from "@/server/authorization";
import { handbookList } from "@/server/handbook";
import { holdingResources } from "@/server/runtime";
import { ArrowUpRight,BookOpen,Sparkles } from "lucide-react";
import Link from "next/link";
import styles from "./learn.module.css";

export default async function LearnPage({params,searchParams}:{params:Promise<{holdingSlug:string}>;searchParams:Promise<{q?:string;cursor?:string}>}) {
  const actor=await requireWorkContext((await params).holdingSlug),base=workspaceBase(publicScope(actor)),query=await searchParams;
  const {write}=await holdingResources(actor.holdingId);
  const knowledge=await handbookList(write,actor,query.q??"",false,query.cursor);
  const permissions:Record<string,string>={finance:"finance.read",commerce:"commerce.read",people:"people.read",settings:"ai.manage"};
  const guides=Object.entries(pageGuides).filter(([path,guide])=>path!=="learn"&&(!permissions[path]||hasPermission(actor,permissions[path]))&&(!query.q||`${guide.title} ${guide.purpose} ${guide.prompts.join(" ")}`.toLowerCase().includes(query.q.toLowerCase())));
  return <FeaturePage className="content" data-source="live">
    <PageHeader eyebrow="Learn in the flow of work" title="Build confidence, one task at a time" description="Practical workflow guides and published company knowledge, close to the work you need to do." action={knowledge.access.editor?<Link className="button secondary" href={`${base}/handbook?edit=1`}>Manage company knowledge</Link>:undefined}/>{!actor.beta&&<PeopleNavigation base={base} actor={actor} scopeKind={actor.scope.kind} active="learn"/>}<WidgetGrid pageKey="learn">
    <FeatureWidget id="learn-open-the-work-ask-what-comes-next-" title="Open the work. Ask what comes next."><div className={styles.symbol}><Sparkles size={27}/></div><div><span className={styles.eyebrow}>YOUR ON-PAGE COACH</span><h2>Open the work. Ask what comes next.</h2><p>Choose a workflow below, then open <strong>Ask ERPX → Teach me</strong> for its steps. Ask in any language and receive clear English guidance. AI uses the current page’s supported sources and your selected role.</p></div><Link href={`${base}/work`} className="button">Open my work <ArrowUpRight size={16}/></Link></FeatureWidget>
    <ListFilters action={`${base}/learn`} query={query.q} placeholder="Search workflows and company knowledge"/>
    <FeatureWidget id="learn-choose-your-workflow" title="Choose your workflow"><div className={styles.sectionHeading}><div><span className={styles.eyebrow}>GET SOMETHING DONE</span><h2 id="workflow-heading">Choose your workflow</h2></div><span className={styles.muted}>{guides.length} guides · Available without AI</span></div>
      {!guides.length&&<EmptyState title="No matching workflows" description="Try another topic or search your company handbook below."/>}
      <div className={styles.grid}>{guides.map(([path,guide])=><article key={path} className={styles.guide}><div className={styles.cardTop}><BookOpen size={20}/><span>{guide.steps.length} steps</span></div><h3>{guide.title}</h3><p>{guide.purpose}</p><ol>{guide.steps.map(step=><li key={step}>{step}</li>)}</ol><Link href={`${base}/${path}`}>Open {guide.title.toLowerCase()} <ArrowUpRight size={16}/></Link></article>)}</div>
    </FeatureWidget>
    <FeatureWidget id="learn-published-knowledge" title="Published knowledge"><div className={styles.sectionHeading}><div><span className={styles.eyebrow}>YOUR COMPANY’S WAY OF WORKING</span><h2 id="knowledge-heading">Published knowledge</h2></div><Link href={`${base}/handbook`}>Browse handbook →</Link></div><p className={styles.muted}>Policies, SOPs, FAQs and service guides published by your company. Open an article and ask ERPX to explain that revision.</p>
      {!knowledge.articles.length?<EmptyState title="No published knowledge matches" description={knowledge.access.editor?"Create and publish an article in Company handbook to make reviewed guidance available here.":"Published guidance will appear here when a company editor makes it available."}/>:<div className={styles.knowledge}>{knowledge.articles.map(article=><Link key={article.id} href={`${base}/handbook/${article.id}`}><span className={styles.eyebrow}>{article.category.replaceAll("_"," ")} · Revision {article.revision}</span><h3>{article.title}</h3><p>{article.excerpt}</p><span>Read & ask AI →</span></Link>)}</div>}
      <ListPagination {...knowledge} query={query} count={knowledge.articles.length}/>
    </FeatureWidget>
  </WidgetGrid></FeaturePage>;
}
