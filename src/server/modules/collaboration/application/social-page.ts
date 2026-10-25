import type { WorkContext } from "../../identity_access/public/authorization";
import type { selfServiceSeed } from "../../records/public/self-service";
import type { homePosts, postView, socialAudienceOptions } from "../public/social-home";

export interface SocialPagePorts {
  prepare(): Promise<void>;
  seed(): ReturnType<typeof selfServiceSeed>;
  post(id: string): ReturnType<typeof postView>;
  feed(filter: { limit: number; stream: "conversations" | "milestones"; tag?: string }): ReturnType<typeof homePosts>;
  audience(): ReturnType<typeof socialAudienceOptions>;
  moderate(): Promise<boolean>;
}
export function createSocialPageQuery(ports: SocialPagePorts) {
  return async (actor: Pick<WorkContext, "beta">, input: { postId?: string; tag?: string; skipDay?: boolean; skipMilestones?: boolean }) => {
    await ports.prepare();
    const initialDay = !input.skipDay && !input.postId && !actor.beta ? ports.seed().catch(() => null) : undefined;
    const [posts, milestones, options, moderate] = await Promise.all([
      input.postId ? ports.post(input.postId).then(post => [post]) : ports.feed({ limit: 8, stream: "conversations", tag: input.tag }),
      input.postId || input.skipMilestones ? Promise.resolve([]) : ports.feed({ limit: 8, stream: "milestones", tag: input.tag }),
      ports.audience(), ports.moderate(),
    ]);
    return { posts, milestones, options, moderate, initialDay, post: input.postId ? posts[0] : null };
  };
}
