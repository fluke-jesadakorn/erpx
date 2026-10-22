import {z} from 'zod';
import {BusinessRuleError} from '@erpx/server-shared/index';
const amount=z.string().regex(/^\d+(\.\d{1,2})?$/),version=z.number().int().positive(),reason=z.string().trim().min(5).max(1000);
export const priceListSchema=z.object({name:z.string().trim().min(3).max(160),currency:z.string().length(3).transform(v=>v.toUpperCase()),customerId:z.string().min(1).nullable().default(null),validFrom:z.iso.date(),validUntil:z.iso.date(),maxAutoDiscountBps:z.number().int().min(0).max(10000),items:z.array(z.object({productId:z.string().min(1),price:amount,taxBps:z.number().int().min(0).max(10000)})).min(1).max(500)});
export const priceListUpdateSchema=priceListSchema.extend({expectedVersion:version});
export const priceListDecisionSchema=z.object({expectedVersion:version,action:z.enum(['activate','cancel']),reason});


export type PriceListInput=z.infer<typeof priceListSchema>;
export type PriceListUpdate=z.infer<typeof priceListUpdateSchema>;
export type PriceListDecision=z.infer<typeof priceListDecisionSchema>;
export type PriceListRow=Record<string,string|number|null>;
export function validatePriceList(input:PriceListInput){
 if(input.validUntil<input.validFrom)throw new BusinessRuleError(422,'PRICE_LIST_DATES_INVALID','The end date must be on or after the start date.');
 if(new Set(input.items.map(i=>i.productId)).size!==input.items.length)throw new BusinessRuleError(422,'PRICE_LIST_PRODUCT_DUPLICATE','Each product may appear once.');
}
