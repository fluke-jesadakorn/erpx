import 'server-only';
import type {SqlDatabase} from '@erpx/platform-runtime-server/public/runtime/types';
import {nativeOwnerOnboardingSlot} from './native-owner-read';
/** The native owner plans the byte-identical onboarding slot read; this
 * adapter only transports the bounded user id and the returned row. */
export function ownerOnboardingSlot(db:SqlDatabase,userId:string){
  return nativeOwnerOnboardingSlot(db,userId);
}
