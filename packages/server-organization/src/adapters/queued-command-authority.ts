import 'server-only';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
/** Source locks only, before an irreversible owner's first snapshot. */
export async function lockQueuedOrganizationAuthority(database:SqlDatabase){await database.prepare('LOCK TABLE organization_nodes,authority_seats,organization_positions IN SHARE MODE').run();}
