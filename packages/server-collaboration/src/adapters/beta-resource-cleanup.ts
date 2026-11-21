import { nativeBetaResourceCleanup } from "@erpx/server-collaboration/adapters/native-beta-resource-cleanup";
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
export async function expireBetaAiReservations(db:SqlDatabase,holdingId:string,pendingBefore:number,now:number){const native=nativeBetaResourceCleanup(db,holdingId);await db.batch([await native.usage(now),await native.draft(pendingBefore,now)]);}
