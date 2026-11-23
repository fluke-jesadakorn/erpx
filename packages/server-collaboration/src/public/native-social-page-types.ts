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
export type { HomePost } from '@erpx/server-collaboration/adapters/social-home';
