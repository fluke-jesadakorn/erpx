// Canonical presentation types for native social-page consumers. Kept separate
// from the legacy public/social-home barrel so UI consumers survive its retirement.
export type {
  PostContent,
  PostPurpose,
  PostAudience,
  PostSource,
  PostDraft,
} from '@erpx/server-collaboration/domain/social-home';
export type { PostRow } from '@erpx/server-collaboration/domain/social-source';
export type {
  HomePost,
  socialAudienceOptions,
  socialPresentation,
} from '@erpx/server-collaboration/adapters/social-home';

import type {
  HomePost,
  socialAudienceOptions,
  socialPresentation,
} from '@erpx/server-collaboration/adapters/social-home';

/** The full SSR page payload. The native reader supplies the complete aggregate
 *  so the presenter never merges it with another authority. */
export type SocialPageData = {
  posts: HomePost[];
  milestones: HomePost[];
  options: Awaited<ReturnType<typeof socialAudienceOptions>>;
  moderate: boolean;
  sourceOnly: boolean;
  presentation: Awaited<ReturnType<typeof socialPresentation>> | undefined;
  initialDay: unknown;
  post: HomePost | null;
};
