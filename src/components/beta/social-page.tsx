
import { PageContent } from "@/components/ui/patterns";
import { selfServiceSeed } from "@/server/self-service";
import { SocialHome } from "./social-home";
import { homePosts, postView, socialAudienceOptions } from "@/server/social-home";
import { holdingResources } from "@/server/runtime";
import { publicScope, type WorkContext } from "@/server/authorization";
import { workspaceBase } from "@/lib/workspace";
import { companyAuthority } from "@/server/staff-policy";
import { RecordDocumentPanel } from "@/components/attachments/record-document-panel";
import { hasPermission } from "@/lib/permissions";
const initialFeedSize = 8;
export async function SocialPage({
  actor,
  postId,
  tag,
}: {
  actor: WorkContext;
  postId?: string;
  tag?: string;
}) {
  const { write } = await holdingResources(actor.holdingId);
  const base = workspaceBase(publicScope(actor));
  const initialDay = !postId && !actor.beta ? selfServiceSeed(actor).catch(() => null) : undefined;
  const [posts, milestones, options, moderate] = await Promise.all([
    postId
      ? postView(write, actor, postId).then((p) => [p])
      : homePosts(write, actor, { limit: initialFeedSize, stream: "conversations", tag }),
    postId ? Promise.resolve([]) : homePosts(write, actor, { limit: initialFeedSize, stream: "milestones", tag }),
    socialAudienceOptions(write, actor),
    companyAuthority(write, actor, ["holding_owner"]),
  ]);
  const post=postId?posts[0]:null;
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
