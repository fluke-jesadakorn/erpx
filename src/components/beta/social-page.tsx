
import { PageContent } from "@/components/ui/patterns";
import { SocialHome } from "./social-home";
import { socialPageData } from "@/server/modules/collaboration/public/social-page";
import { publicScope, type WorkContext } from "@/server/modules/identity_access/public/authorization";
import { workspaceBase } from "@/lib/workspace";
import { RecordDocumentPanel } from "@/components/attachments/record-document-panel";
import { hasPermission } from "@/lib/permissions";
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
  const { posts, milestones, options, moderate, initialDay, post } = await socialPageData(actor, { postId, tag });
  return (
    <>
    <SocialHome
      key={`${base}:${actor.companyId}:${actor.userId}`}
      base={base}
      userId={actor.userId}
      name={actor.name}
      branchId={actor.branchId}
      canModerate={moderate}
      canRequestLegal={hasPermission(actor, "legal.create")}
      initialPosts={posts}
      initialMilestones={milestones}
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
      initialDay={initialDay}
    />
    {post&&<PageContent ><RecordDocumentPanel appearance="standard"
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
    /></PageContent>}
    </>
  );
}
