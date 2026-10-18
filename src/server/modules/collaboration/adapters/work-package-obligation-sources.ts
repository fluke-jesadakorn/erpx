import 'server-only';
import type {SqlDatabase} from '../../runtime/public/runtime/types';
/** Actual native social provenance, shared by Work task and social observers.
 * PMS/system records are explicitly separate from ordinary Work posts. */
export const workSocialSettlementSources={origins:'SELECT * FROM social_task_origins',posts:'SELECT * FROM social_posts',postSources:'SELECT * FROM social_post_sources'} as const;
export function lockWorkSocialSettlementSources(database:SqlDatabase){return database.prepare('LOCK TABLE social_task_origins,social_posts,social_post_sources IN SHARE MODE').run();}
