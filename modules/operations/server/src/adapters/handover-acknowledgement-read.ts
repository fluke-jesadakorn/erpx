import 'server-only';
import type {SqlDatabase} from '@erpx/platform-runtime-server/public/runtime/types';
import type {StaffActor} from '@erpx/operations-server/public/staff-policy';
import {readableHandoverReceipts} from '@erpx/collaboration-server/public/handover-receipts';
/** Initial receipt content uses the actual Social source, never a Work
 * snapshot or a synthetic full Work/People grant. */
export function handoverAcknowledgements(database:SqlDatabase,actor:StaffActor,postIds:readonly string[]){return readableHandoverReceipts(database,actor,postIds);}
