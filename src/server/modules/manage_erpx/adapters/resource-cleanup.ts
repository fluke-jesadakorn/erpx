import type {SqlDatabase,FileStore} from '../../runtime/public/runtime/types';
import {createResourceCleanupApplication} from '../application/resource-cleanup';
import {betaUploadCleanup} from '../../files/public/beta-resource-cleanup';
import {expireBetaAiReservations} from '../../collaboration/public/beta-resource-cleanup';
/** Bounded cleanup: expired intents cannot finalize and their uncommitted objects are safe to remove. */
export async function cleanupBetaResources(db:SqlDatabase,files:FileStore,holdingId:string){return createResourceCleanupApplication({clock:{now:()=>Date.now()},objects:{delete:async keys=>{await files.delete(keys);}},repository:{...betaUploadCleanup(db),expireAiReservations:(pendingBefore,now)=>expireBetaAiReservations(db,pendingBefore,now)}}).cleanup(holdingId);}
