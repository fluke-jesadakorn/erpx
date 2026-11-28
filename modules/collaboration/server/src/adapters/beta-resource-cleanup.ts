import { nativeBetaResourceCleanup } from "@erpx/collaboration-server/adapters/native-beta-resource-cleanup";
import type {SqlDatabase} from '@erpx/platform-runtime-server/public/runtime/types';
export async function expireBetaAiReservations(db:SqlDatabase,holdingId:string,pendingBefore:number,now:number){const native=nativeBetaResourceCleanup(db,holdingId);await db.batch([await native.usage(now),await native.draft(pendingBefore,now)]);}
