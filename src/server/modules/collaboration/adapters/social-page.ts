import "server-only";
import { holdingResources } from "../../runtime/public/runtime";
import type { SqlDatabase } from "../../runtime/public/runtime/types";
import type { WorkContext } from "../../identity_access/public/authorization";
import { selfServiceSeed } from "../../records/public/self-service";
import { companyAuthority } from "../../operations/public/staff-policy";
import { homePosts, postView, socialAudienceOptions } from "../public/social-home";
import { createSocialPageQuery } from "../application/social-page";

export function socialPageData(actor: WorkContext, input: { postId?: string; tag?: string; skipDay?: boolean; skipMilestones?: boolean }) {
  let db: SqlDatabase;
  return createSocialPageQuery({
    prepare: async () => { db = (await holdingResources(actor.holdingId)).write; },
    seed: () => selfServiceSeed(actor),
    post: id => postView(db, actor, id),
    feed: filter => homePosts(db, actor, filter),
    audience: () => socialAudienceOptions(db, actor),
    moderate: () => companyAuthority(db, actor, ["holding_owner"]),
  })(actor, input);
}
