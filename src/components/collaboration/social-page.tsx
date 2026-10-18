import { HomeContextBoundary } from "@/components/work_projection/home/home-context-boundary";
import type { WorkSnapshot } from "@/server/modules/work_projection/public/work-items";
import { Suspense } from "react";
import { SocialHomeShell } from "@/components/collaboration/social-home-shell";
import { homeContextKey } from "@/server/modules/work_projection/public/home-cache";
import { measureReadPhase } from "@/server/modules/runtime/public/runtime/read-metrics";
import styles from "@/components/collaboration/social.module.css";

import { PageContent } from "@/components/ui/patterns";
import {SocialCommandProvider} from "./social-command-context";
import { SocialHome } from "@/components/collaboration/social-home";
import { socialPageData } from "@/server/modules/collaboration/public/social-page";
import { publicScope, type WorkContext } from "@/server/modules/identity_access/public/authorization";
import { workspaceBase } from "@/lib/work_projection/workspace";
import { RecordDocumentPanel } from "@/components/files/record-document-panel";
import {socialPostSourceOnly} from '@/server/modules/collaboration/public/product-entry';
import { hasPermission } from "@/lib/identity_access/permissions";
export async function SocialPage({
  actor,
  postId,
  tag,
}: {
  actor: WorkContext;
  postId?: string;
  tag?: string;
  initialWork?: Promise<WorkSnapshot>;
}) {
  const base = workspaceBase(publicScope(actor));
  const contextKey = homeContextKey(actor);
  const sourceOnly=!!postId&&await socialPostSourceOnly(actor,postId);
  const data = measureReadPhase("home_feed", () => socialPageData(actor, { postId, tag, skipDay: true, skipMilestones: true,sourceOnly }));
  return <HomeContextBoundary key={contextKey} contextKey={contextKey}>
  <SocialHomeShell base={base} name={actor.name} single={!!postId} sourceOnly={sourceOnly} calendarEnabled={!actor.beta}
    contextKey={contextKey} canRequestLegal={hasPermission(actor, "legal.create")}>
    <Suspense fallback={<div className={styles.stream} role="status">Loading team updates…</div>}>
      <SocialFeed actor={actor} base={base} postId={postId} sourceOnly={sourceOnly} data={data} />
    </Suspense>
  </SocialHomeShell>
  {postId && <Suspense><SocialDocuments actor={actor} base={base} data={data} /></Suspense>}
  </HomeContextBoundary>;
}

async function SocialFeed({ actor, base, postId, sourceOnly, data }: {
  actor: WorkContext; base: string; postId?: string; sourceOnly:boolean; data: ReturnType<typeof socialPageData>;
}) {
  const { posts, options, moderate, presentation } = await data;
  return (
    <>
    <SocialCommandProvider value={{apiRoot:`/api/v1${base}`,token:presentation?.context??homeContextKey(actor),companyId:actor.companyId,userId:actor.userId}}><SocialHome
      key={homeContextKey(actor)}
      base={base}
      userId={actor.userId}
      name={actor.name}
      branchId={actor.branchId}
      canModerate={moderate}
      purchase={presentation?.purchase}
      initialPosts={posts}
      initialMeta={{
        ...(!sourceOnly?{handoffContext: { companyId: actor.companyId, userId: actor.userId,
          assignmentId: actor.assignmentId, base: workspaceBase(publicScope(actor)) }}:{}),
        people: [],
        branches: options.branches,
        audiences: options.audiences,
        projects: options.projects,
        drafts: [],
        tasks: [],
      }}
      single={!!postId}
      sourceOnly={sourceOnly}
      calendarEnabled={!actor.beta}
    /></SocialCommandProvider>

    </>
  );
}

async function SocialDocuments({ actor, base, data }: {
  actor: WorkContext; base: string; data: ReturnType<typeof socialPageData>;
}) {
  const { post } = await data;
  return <>    {post&&<PageContent ><RecordDocumentPanel appearance="standard"
      base={base}
      targetType="social_post"
      targetId={post.id}
      expectedTargetVersion={Number(post.version)}
      title={post.purpose==='handover'?"Handover files":"Conversation files"}
      editable={post.author_id===actor.userId&&post.purchase?.newWork!==false}
      purposes={[post.purpose==='handover'?{
        key:'handover_file',label:'Handover file',description:'Add a controlled file revision for this handover.',visibility:'Only colleagues who can still read the handover and its source records can open this file.',
      }:{
        key:'thread_file',label:'Conversation file',description:'Add a controlled file revision to this conversation.',visibility:'Access follows the conversation audience and is rechecked on every read.',
      }]}
    /></PageContent>}</>;
}
