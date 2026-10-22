import { z } from "zod";

const key=z.string().trim().min(1).max(100),existingId=z.string().min(1).max(160).optional();
const minor=z.number().int().min(0).max(1_000_000_000);
/** All amounts are THB satang; IDs from the source remain stable within the batch. */
export const pmsImportSchema=z.object({
  propertyId:z.string().min(1).max(160),expectedVersion:z.number().int().positive(),cutoverDate:z.iso.date(),
  source:z.string().trim().min(1).max(100),currency:z.literal("THB"),
  roomTypes:z.array(z.object({key,existingId,name:z.string().min(1).max(200),capacity:z.number().int().min(1).max(50),amenities:z.string().max(2000).default("")})).max(100),
  rooms:z.array(z.object({key,existingId,number:z.string().min(1).max(100),typeKey:key,building:z.string().max(100).default(""),floor:z.string().max(100).default("")})).max(1000),
  rates:z.array(z.object({key,existingId,name:z.string().min(1).max(200),typeKey:key,from:z.iso.date(),to:z.iso.date(),priceMinor:minor.positive()})).max(200),
  guests:z.array(z.object({key,existingId,partyId:z.string().min(1).max(160).optional(),name:z.string().min(1).max(200),email:z.string().max(200).default(""),phone:z.string().max(100).default("")})).max(2000),
  reservations:z.array(z.object({key,guestKey:key,typeKey:key,rateKey:key,roomKey:key.optional(),arrival:z.iso.date(),departure:z.iso.date(),
    adults:z.number().int().min(1).max(50),state:z.enum(["reserved","in_house"]),openingInvoiceId:z.string().min(1).max(160).optional(),
    opening:z.object({due:minor,advance:minor,advanceTax:minor,deposit:minor}).default({due:0,advance:0,advanceTax:0,deposit:0})})).max(500),
}).strict();
export type PmsImportDefinition=z.infer<typeof pmsImportSchema>;
export type PmsImportReport={errors:{row:string;code:string;message:string}[];counts:Record<string,number>;
  totals:{due:number;advance:number;advanceTax:number;deposit:number};canApprove:boolean};
