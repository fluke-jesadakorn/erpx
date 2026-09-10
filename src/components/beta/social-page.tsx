import { workSnapshot, nextWorkSteps } from "@/server/work-items";
import { SocialHome } from "./social-home";
import { homePosts, postView } from "@/server/social-home";
import { holdingResources } from "@/server/runtime";
import { publicScope, type WorkContext } from "@/server/authorization";
import { workspaceBase } from "@/lib/workspace";
import { staffPeople, companyAuthority } from "@/server/staff-policy";
import { listTasks } from "@/server/staff-tasks";
export async function SocialPage({
  actor,
  postId,
}: {
  actor: WorkContext;
  postId?: string;
}) {
  const { write } = await holdingResources(actor.holdingId);
  const [posts, people, branches, drafts, tasks, moderate] = await Promise.all([
    postId
      ? postView(write, actor, postId).then((p) => [p])
      : homePosts(write, actor),
    staffPeople(write, actor.companyId),
    write
      .prepare(
        "SELECT id,display_name FROM organization_nodes WHERE kind='branch' AND company_id=? AND status='active' AND (?::text IS NULL OR id=?)",
      )
      .bind(actor.companyId, actor.branchId, actor.branchId)
      .all<{ id: string; display_name: string }>(),
    write
      .prepare(
        "SELECT id,body,branch_id,version,mentions_json FROM social_post_drafts WHERE author_id=? AND company_id=? AND published_post_id IS NULL ORDER BY updated_at DESC LIMIT 20",
      )
      .bind(actor.userId, actor.companyId)
      .all<{
        id: string;
        body: string;
        branch_id: string | null;
        version: number;
        mentions_json: string;
      }>(),
    listTasks(write, actor),
    companyAuthority(write, actor, ["holding_owner"]),
  ]);
  const work = postId ? null : await workSnapshot(actor);
  return (
    <SocialHome
      base={workspaceBase(publicScope(actor))}
      userId={actor.userId}
      name={actor.name}
      branchId={actor.branchId}
      canModerate={moderate}
      initialPosts={posts}
      nextSteps={work ? nextWorkSteps(work.items) : []}
      unavailableWork={work?.unavailableSources ?? []}
      initialMeta={{
        people: people.map((p) => ({
          id: p.user_id,
          name: p.legal_name,
          branchId: p.branch_id,
        })),
        branches: branches.results,
        drafts: drafts.results,
        tasks,
      }}
      single={!!postId}
    />
  );
}
