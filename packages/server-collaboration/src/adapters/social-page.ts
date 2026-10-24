import "server-only";
import { holdingResources } from "../../../../packages/server-runtime/src/public/runtime";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import { selfServiceSeed } from "@erpx/server-records/public/self-service";
import { companyAuthority } from "@erpx/server-operations/public/staff-policy";
import {homePosts,postView,socialAudienceOptions,socialPresentation} from '@erpx/server-collaboration/public/social-home';
import { createSocialPageQuery } from "@erpx/server-collaboration/application/social-page";

export function socialPageData(actor: WorkContext, input: { postId?: string; tag?: string; skipDay?: boolean; skipMilestones?: boolean; sourceOnly?: boolean }) {
  let db: SqlDatabase;
  return createSocialPageQuery({
    prepare: async () => { db = (await holdingResources(actor.holdingId)).write; },
    seed: () => selfServiceSeed(actor),
    post: id => postView(db, actor, id),
    feed: filter => homePosts(db, actor, filter),
    audience: () => socialAudienceOptions(db, actor),
    presentation:()=>socialPresentation(db,actor),
    moderate: () => companyAuthority(db, actor, ["holding_owner"]),
  })(actor, input);
}
