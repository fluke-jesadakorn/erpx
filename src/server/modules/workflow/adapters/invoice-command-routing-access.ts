import 'server-only';
import type {WorkContext} from '../../identity_access/public/authorization';
import type {SqlDatabase} from '../../runtime/public/runtime/types';
import {resolveFlow} from './flow-routing';

/** Presentation only: retain the original owner's current invoice responsibility
 * and eligibility policy. The command repeats that policy in its own unit. */
export async function invoiceCommandRoutingCanAct(database:SqlDatabase,actor:WorkContext,invoiceId:string):Promise<boolean>{
 return (await resolveFlow(database,actor,'invoice',invoiceId)).canAct;
}
