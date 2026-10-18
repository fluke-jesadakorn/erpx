import 'server-only';
import type {SqlDatabase} from '../../runtime/public/runtime/types';
import {lockScheduledReleaseSources} from '../../platform_administration/public/release-source-locks';
/** Original native scheduledFeaturePredicate sources, before any owner snapshot.
 * Disabled/development release behavior is unchanged. */
export async function lockScheduledFeatureSources(database:SqlDatabase){if(process.env.NODE_ENV!=='production')return;await database.prepare('LOCK TABLE company_feature_rollouts IN SHARE MODE').run();await lockScheduledReleaseSources(database);}
