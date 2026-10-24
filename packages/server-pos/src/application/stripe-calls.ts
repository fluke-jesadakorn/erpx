import{z}from 'zod';
import type{StripeOptions}from '@erpx/server-pos/domain/stripe';
export const objectSchema = z.object({ id: z.string() }).passthrough();
export type StripeObject = z.infer<typeof objectSchema>;
export function createStripeCalls(provider:{request(live:boolean,path:string,options?:StripeOptions):Promise<unknown>}){
    async function stripeCall(live: boolean, path: string, options: StripeOptions = {}): Promise<StripeObject> { return objectSchema.parse(await provider.request(live, path, options)); }
    async function stripeListCall(live: boolean, path: string, options: StripeOptions = {}) { return z.object({ object: z.literal('list'), data: z.array(z.record(z.string(), z.unknown())), has_more: z.boolean() }).parse(await provider.request(live, path, options)); }

return{stripeCall,stripeListCall};}
