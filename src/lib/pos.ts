import { z } from 'zod';
import { scaledAmount } from './commerce-math';
import { APP_LOCALE } from './localization';
export const money = z.number().int().min(0).max(99999999999);
export const qty = z.number().int().positive().max(999999999);
export const componentSchema = z.object({productId:z.string().min(1), quantityMilli:qty,unitLabel:z.string().trim().max(40).optional(),conversionMicros:z.number().int().positive().max(1000000000000).optional()});
export const modifierSchema = z.object({id:z.string().min(1),name:z.string().min(1).max(100),priceMinor:money,ingredients:z.array(componentSchema).max(30).default([])});
export const accountKeys = ['cash','bank','edc','stripe','receivable','revenue','outputTax','inventory','cogs','advance','wip','waste','refund','fees','cashDifference','cashTransfer','disputes'] as const;
export type PosAccountKey = typeof accountKeys[number];
export type PosAccounts = Record<PosAccountKey,string> & {chartId:string};
export const accountsSchema = z.object(Object.fromEntries([...accountKeys,'chartId'].map(k=>[k,z.string().min(1)])) as Record<PosAccountKey|'chartId',z.ZodString>);
export const settingsSchema=z.object({enabled:z.boolean(),currency:z.string().regex(/^[A-Z]{3}$/),timezone:z.literal('Asia/Bangkok'),cashierDiscountBps:z.number().int().min(0).max(10000),accounts:accountsSchema,expectedVersion:z.number().int().min(0)});
export const catalogSchema=z.object({productId:z.string().min(1),barcode:z.string().trim().max(100).default(''),category:z.string().max(100).default(''),kind:z.enum(['stock','recipe','service']),station:z.string().min(1).max(100),recipe:z.array(componentSchema).max(50),modifiers:z.array(modifierSchema).max(30),taxInclusive:z.boolean(),expectedVersion:z.number().int().min(0),enabled:z.boolean().default(true),registerIds:z.array(z.string().min(1)).max(100).optional(),imageId:z.string().nullable().optional()});
export type PaymentMethod='cash'|'transfer'|'edc'|'stripe_card'|'stripe_promptpay';
export function saleAmounts(quantity:number, price:number,taxBps:number,inclusive:boolean,discount=0){
 const gross=scaledAmount(price,quantity,1000);
 if(discount>gross)throw new Error('DISCOUNT_EXCEEDS_PRICE');
 const discounted=gross-discount;
 const subtotal=inclusive?scaledAmount(discounted,10000,10000+taxBps):discounted;
 const tax=inclusive?discounted-subtotal:scaledAmount(subtotal,taxBps,10000);
 return {subtotal,tax,total:subtotal+tax,discount};
}
/** Cumulative allocation assigns the final rounding remainder exactly once. */
export function portion(total:number,previous:number,quantity:number,whole:number){
 if(previous<0||quantity<=0||previous+quantity>whole)throw new Error('QUANTITY_EXCEEDED');
 return scaledAmount(total,previous+quantity,whole)-scaledAmount(total,previous,whole);
}
export function recipeQuantities(recipe:Array<{productId:string;quantityMilli:number;conversionMicros?:number}>,quantity:number){
 const totals=new Map<string,number>();
 for(const part of recipe){const amount=scaledAmount(scaledAmount(part.quantityMilli,part.conversionMicros??1000000,1000000),quantity,1000);if(!amount)throw new Error('INGREDIENT_BELOW_PRECISION');totals.set(part.productId,(totals.get(part.productId)??0)+amount);}
 return [...totals].sort(([a],[b])=>a.localeCompare(b)).map(([productId,quantityMilli])=>({productId,quantityMilli}));
}
export function businessDate(timezone='Asia/Bangkok',date=new Date()) {return new Intl.DateTimeFormat('en-CA',{timeZone:timezone,year:'numeric',month:'2-digit',day:'2-digit'}).format(date);}
// Preserve existing two-decimal POS amounts; never relabel them with a global default.
export function formatPosMoney(n:number,currency:string|null){return currency ? new Intl.NumberFormat(APP_LOCALE,{style:'currency',currency,currencyDisplay:'code'}).format(n/100) : '—';}
