import type {SqlDatabase,FileStore} from '@erpx/server-runtime/public/runtime/types';
import {createResourceCleanupApplication} from '@erpx/server-manage_erpx/application/resource-cleanup';
import {betaUploadCleanup} from '@erpx/server-files/public/beta-resource-cleanup';
import {expireBetaAiReservations} from '@erpx/server-collaboration/public/beta-resource-cleanup';
/** Bounded cleanup: expired intents cannot finalize and their uncommitted objects are safe to remove. */
export async function cleanupBetaResources(db:SqlDatabase,files:FileStore,holdingId:string){return createResourceCleanupApplication({clock:{now:()=>Date.now()},objects:{delete:async keys=>{await files.delete(keys);}},repository:{...betaUploadCleanup(db),expireAiReservations:(pendingBefore,now)=>expireBetaAiReservations(db,pendingBefore,now)}}).cleanup(holdingId);}
