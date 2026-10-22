import {z} from 'zod';
export const portalSupportResponseSchema=z.object({expectedBindingVersion:z.number().int().positive(),expectedResponseVersion:z.number().int().nonnegative(),message:z.string().trim().min(3).max(4000),reason:z.string().trim().min(3).max(2000)});
export const portalSupportAcknowledgementSchema=z.object({bindingId:z.string().min(1).max(200),expectedBindingVersion:z.number().int().positive(),responseId:z.string().min(1).max(200),responseVersion:z.number().int().positive(),note:z.string().trim().max(500).default('')});
export const portalSupportClosureSchema=z.object({expectedBindingVersion:z.number().int().positive(),expectedResponseVersion:z.number().int().nonnegative(),reason:z.string().trim().min(3).max(2000)});
export type PortalSupportResponseInput=z.infer<typeof portalSupportResponseSchema>;
export type PortalSupportAcknowledgementInput=z.infer<typeof portalSupportAcknowledgementSchema>;
export type PortalSupportThread={requestId:string;requestedAt:number;note:string;response:null|{id:string;version:number;message:string;publishedAt:number};acknowledgedAt:number|null;closedAt:number|null};
