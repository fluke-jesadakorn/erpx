import { HomeContextBoundary } from "@erpx/feature-work_projection/components/home/home-context-boundary";
import { Suspense } from "react";
import { SocialHomeShell } from "@erpx/feature-collaboration/components/social-home-shell";
import { homeContextKey } from "@erpx/server-work_projection/public/home-cache";
import { measureReadPhase } from "@erpx/server-runtime/public/runtime/read-metrics";
import styles from "@erpx/feature-collaboration/components/social.module.css";

import { InlineNotice } from "@erpx/ui/components/foundation";
import { PageContent } from "@erpx/ui/components/patterns";
import {SocialCommandProvider} from "@erpx/feature-collaboration/components/social-command-context";
import { SocialHome } from "@erpx/feature-collaboration/components/social-home";
import { socialPageData } from "@erpx/server-collaboration/public/social-page";
import type { SocialPageData } from "@erpx/server-collaboration/public/native-social-page-types";
import { publicScope, type WorkContext } from "@erpx/server-identity_access/public/authorization";
import { workspaceBase } from "@erpx/feature-work_projection/lib/workspace";
import { RecordDocumentPanel } from "@erpx/feature-files/components/record-document-panel";
import { hasPermission } from "@erpx/feature-identity_access/lib/permissions";
import { preferredAccountLanguage } from "@erpx/server-identity_access/public/account-preference-snapshot";
import { ApiError } from "@erpx/server-runtime/public/http";
export async function SocialPage({
  actor,
  postId,
  tag,
}: {
  actor: WorkContext;
  postId?: string;
  tag?: string;
}) {
  const base = workspaceBase(publicScope(actor));
  const contextKey = homeContextKey(actor);
  const language = await preferredAccountLanguage("en");
  const data = measureReadPhase("home_feed", () => socialPageData(actor, { postId, tag, skipDay: true, skipMilestones: true }));
  const sourceOnly = !!postId && (await data).sourceOnly;
  return <HomeContextBoundary key={contextKey} contextKey={contextKey}>
  <SocialHomeShell base={base} name={actor.name} single={!!postId} sourceOnly={sourceOnly} calendarEnabled={!actor.beta} language={language}
    contextKey={contextKey} canRequestLegal={hasPermission(actor, "legal.create")}>
    <Suspense fallback={<div className={styles.stream} role="status">Loading team updates…</div>}>
      <SocialFeed actor={actor} base={base} postId={postId} sourceOnly={sourceOnly} data={data} language={language} />
    </Suspense>
  </SocialHomeShell>
  {postId && <Suspense><SocialDocuments actor={actor} base={base} data={data} /></Suspense>}
  </HomeContextBoundary>;
}

async function SocialFeed({ actor, base, postId, sourceOnly, data, language }: {
  actor: WorkContext;
  base: string;
  postId?: string;
  sourceOnly: boolean;
  data: Promise<SocialPageData>;
  language: "en" | "th";
}) {
  const page = await data.catch(error => {
    if (!postId && error instanceof ApiError && error.status === 403 && error.code === "MODULE_UNAVAILABLE") return null;
    throw error;
  });
  if (!page) {
    return <div className={styles.stream}>
      <InlineNotice tone="info">
        {language === "th"
          ? "บทบาทนี้ไม่สามารถอ่านบทสนทนาของทีมในพื้นที่นี้ได้ คุณยังเปิดแอปจากเมนู Apps ได้"
          : "This role can’t read team conversations in this workspace. You can still open your tools from Apps."}
      </InlineNotice>
    </div>;
  }
  const { posts, options, moderate, presentation } = page;
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
  actor: WorkContext; base: string; data: Promise<SocialPageData>;
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
