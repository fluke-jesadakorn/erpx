import {z} from 'zod';export const profileSchema = z.object({
  chartId: z.string().min(1), inventoryAccountId: z.string().min(1),
  wipAccountId: z.string().min(1), laborAccountId: z.string().min(1), overheadAccountId: z.string().min(1), scrapAccountId: z.string().min(1),
});
export const reviewSchema = z.object({
  expectedVersion: z.number().int().positive(), action: z.enum(["approve", "reject", "retire"]),
  reason: z.string().trim().min(3).max(3000),
});

export function manufacturingAccountKind(index:number):'asset'|'scrap_expense'|'conversion'{return index<2?'asset':index===4?'scrap_expense':'conversion';}
