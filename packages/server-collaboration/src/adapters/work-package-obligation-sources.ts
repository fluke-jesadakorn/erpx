import 'server-only';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {settlementSourceRows} from './read-tail-sql';
/** Actual native social provenance, shared by Work task and social observers.
 * PMS/system records are explicitly separate from ordinary Work posts. */
export const workSocialSettlementSources={origins:settlementSourceRows('social_task_origins'),posts:settlementSourceRows('social_posts'),postSources:settlementSourceRows('social_post_sources')} as const;
export function lockWorkSocialSettlementSources(database:SqlDatabase){return database.prepare('LOCK TABLE social_task_origins,social_posts,social_post_sources IN SHARE MODE').run();}
