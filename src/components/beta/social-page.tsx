import { SocialHome } from "./social-home";
import { homePosts, postView } from "@/server/social-home";
import { holdingResources } from "@/server/runtime";
import { publicScope, type WorkContext } from "@/server/authorization";
import { workspaceBase } from "@/lib/workspace";
import { companyAuthority } from "@/server/staff-policy";
const initialFeedSize = 8;
export async function SocialPage({
  actor,
  postId,
}: {
  actor: WorkContext;
  postId?: string;
}) {
  const { write } = await holdingResources(actor.holdingId);
  const [posts, branches, moderate] = await Promise.all([
    postId
      ? postView(write, actor, postId).then((p) => [p])
      : homePosts(write, actor, { limit: initialFeedSize }),
    write
      .prepare(
        "SELECT id,display_name FROM organization_nodes WHERE kind='branch' AND company_id=? AND status='active' AND (?::text IS NULL OR id=?)",
      )
      .bind(actor.companyId, actor.branchId, actor.branchId)
      .all<{ id: string; display_name: string }>(),
    companyAuthority(write, actor, ["holding_owner"]),
  ]);
  return (
    <SocialHome
      key={`${workspaceBase(publicScope(actor))}:${actor.companyId}:${actor.userId}`}
      base={workspaceBase(publicScope(actor))}
      userId={actor.userId}
      name={actor.name}
      branchId={actor.branchId}
      canModerate={moderate}
      initialPosts={posts}
      initialMeta={{
        people: [],
        branches: branches.results,
        drafts: [],
        tasks: [],
      }}
      single={!!postId}
    />
  );
}
