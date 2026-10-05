import 'server-only';
import {z} from 'zod';
import {bookingEnv} from '../pms-booking';
import {publicChangeQuote,confirmBookingChange} from '../pms-booking-changes';
import type {WorkContext} from '../authorization';
import type {TransactionalSqlDatabase} from '../runtime/types';

export async function staffBookingChange(db:TransactionalSqlDatabase,actor:WorkContext,slug:string,raw:unknown,key:string,now=Date.now()){
 const value=z.object({action:z.enum(['change-quote','cancel-quote','change-confirm','cancel-confirm']),reservationId:z.string().min(1).max(128),reason:z.string().trim().min(3).max(2000),override:z.boolean().default(false),items:z.unknown().optional(),proposalToken:z.string().max(100000).optional()}).strict().parse(raw);
 const authority={actor,reservationId:value.reservationId,reason:value.reason,override:value.override},kind=value.action.startsWith('cancel')?'cancel':'change',env=await bookingEnv();
 if(value.action.endsWith('quote'))return publicChangeQuote(db,'',kind,{items:value.items},env,now,authority);
 return confirmBookingChange(db,slug,'',z.string().min(1).parse(value.proposalToken),key,env,now,authority,kind);
}
