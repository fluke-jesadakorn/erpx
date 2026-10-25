import { HomeContextBoundary } from "@/components/home/home-context-boundary";
import type { WorkSnapshot } from "@/server/modules/work_projection/public/work-items";
import { Suspense } from "react";
import { SocialHomeShell } from "./social-home-shell";
import { homeContextKey } from "@/server/modules/work_projection/public/home-cache";
import { measureReadPhase } from "@/server/modules/runtime/public/runtime/read-metrics";
import styles from "./social.module.css";

import { PageContent } from "@/components/ui/patterns";
import { SocialHome } from "./social-home";
import { socialPageData } from "@/server/modules/collaboration/public/social-page";
import { publicScope, type WorkContext } from "@/server/modules/identity_access/public/authorization";
import { workspaceBase } from "@/lib/workspace";
import { RecordDocumentPanel } from "@/components/attachments/record-document-panel";
import { hasPermission } from "@/lib/permissions";
export function SocialPage({
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
  const data = measureReadPhase("home_feed", () => socialPageData(actor, { postId, tag, skipDay: true, skipMilestones: true }));
  return <HomeContextBoundary key={contextKey} contextKey={contextKey}>
  <SocialHomeShell base={base} name={actor.name} single={!!postId} calendarEnabled={!actor.beta}
    contextKey={contextKey} canRequestLegal={hasPermission(actor, "legal.create")}>
    <Suspense fallback={<div className={styles.stream} role="status">Loading team updates…</div>}>
      <SocialFeed actor={actor} base={base} postId={postId} data={data} />
    </Suspense>
  </SocialHomeShell>
  {postId && <Suspense><SocialDocuments actor={actor} base={base} data={data} /></Suspense>}
  </HomeContextBoundary>;
}

async function SocialFeed({ actor, base, postId, data }: {
  actor: WorkContext; base: string; postId?: string; data: ReturnType<typeof socialPageData>;
}) {
  const { posts, options, moderate } = await data;
  return (
    <>
    <SocialHome
      key={homeContextKey(actor)}
      base={base}
      userId={actor.userId}
      name={actor.name}
      branchId={actor.branchId}
      canModerate={moderate}
      initialPosts={posts}
      initialMeta={{
        handoffContext: { companyId: actor.companyId, userId: actor.userId,
          assignmentId: actor.assignmentId, base: workspaceBase(publicScope(actor)) },
        people: [],
        branches: options.branches,
        audiences: options.audiences,
        projects: options.projects,
        drafts: [],
        tasks: [],
      }}
      single={!!postId}
      calendarEnabled={!actor.beta}
    />

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
      editable={post.author_id===actor.userId}
      purposes={[post.purpose==='handover'?{
        key:'handover_file',label:'Handover file',description:'Add a controlled file revision for this handover.',visibility:'Only colleagues who can still read the handover and its source records can open this file.',
      }:{
        key:'thread_file',label:'Conversation file',description:'Add a controlled file revision to this conversation.',visibility:'Access follows the conversation audience and is rechecked on every read.',
      }]}
    /></PageContent>}</>;
}
