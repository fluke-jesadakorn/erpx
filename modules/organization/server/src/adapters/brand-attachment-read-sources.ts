import 'server-only';
import type {SqlDatabase} from '@erpx/platform-runtime-server/public/runtime/types';

/** Native organization facts for the existing Brand private-file policy.
 * This point port grants neither organization management nor provider access. */
export function brandAttachmentOrganizationSourceLocks(database:SqlDatabase){return [database.prepare('LOCK TABLE organization_nodes IN SHARE ROW EXCLUSIVE MODE')];}
