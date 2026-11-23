import "server-only";
import { holdingResources } from "../../../../packages/server-runtime/src/public/runtime";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import { selfServiceSeed } from "@erpx/server-records/public/self-service";
import { companyAuthority } from "@erpx/server-operations/public/staff-policy";
import {homePosts,postView,socialAudienceOptions,socialPresentation} from '@erpx/server-collaboration/public/social-home';
import { createSocialPageQuery } from "@erpx/server-collaboration/application/social-page";
import {
  nativeSocialAvailable,
  nativeSocialPageDataSSR,
} from "@erpx/server-collaboration/public/native-social-page";
import type { SocialPageData } from "@erpx/server-collaboration/public/native-social-page-types";

type PageInput = {
  postId?: string;
  tag?: string;
  skipDay?: boolean;
  skipMilestones?: boolean;
  sourceOnly?: boolean;
};

/** Retained TypeScript authority. Kept only while the native owner page port
 *  is unconfigured; the native branch returns the identical full shape. */
function retainedPageData(actor: WorkContext, input: PageInput): Promise<SocialPageData> {
  let db: SqlDatabase;
  return createSocialPageQuery({
    prepare: async () => { db = (await holdingResources(actor.holdingId)).write; },
    seed: () => selfServiceSeed(actor),
    post: id => postView(db, actor, id),
    feed: filter => homePosts(db, actor, filter),
    audience: () => socialAudienceOptions(db, actor),
    presentation:()=>socialPresentation(db,actor),
    moderate: () => companyAuthority(db, actor, ["holding_owner"]),
  })(actor, input) as Promise<SocialPageData>;
}

/** Social page SSR data. When the native Collaboration transport is bound,
 *  the complete aggregate page reply is authoritative; no field is merged from
 *  the retained query. The fallback is removed with the social-home barrel. */
export function socialPageData(actor: WorkContext, input: PageInput): Promise<SocialPageData> {
  return nativeSocialAvailable()
    ? nativeSocialPageDataSSR(actor, input)
    : retainedPageData(actor, input);
}
