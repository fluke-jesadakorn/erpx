import {z} from "zod";
export const legalEnvelopeInput=z.object({action:z.literal("signing.send"),id:z.string(),expectedVersion:z.number().int().positive(),expiresAt:z.number().int(),consent:z.string().trim().min(20).max(10000)});
export const legalSigningActionInput=z.object({action:z.enum(["signing.view","signing.sign","signing.decline"]),id:z.string(),expectedVersion:z.number().int().positive(),envelopeId:z.string(),consent:z.string().optional()});
/** This release accepts externally signed originals under independent review. */
export function refuseLegalSigning(error:(status:number,code:string,message:string)=>Error):never{throw error(409,"LEGAL_SIGNING_DISABLED","Signing through ERPX is unavailable in this release. Import the signed original for independent review.");}
