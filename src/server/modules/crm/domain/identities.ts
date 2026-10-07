import {z} from "zod";
const id=z.string().min(1), evidence=z.string().trim().min(3).max(2000);
export const crmIdentityCommand=z.discriminatedUnion('action',[
 z.object({action:z.literal('create-account'),kind:z.enum(['business','individual']),name:z.string().trim().min(2).max(160)}),
 z.object({action:z.literal('create-contact'),accountId:id,name:z.string().trim().min(2).max(160),detail:evidence}),
 z.object({action:z.literal('link-finance'),accountId:id,customerId:id,expectedVersion:z.number().int().positive(),evidence}),
 z.object({action:z.literal('link-guest'),contactId:id,guestId:id,propertyId:id,evidence}),
]);

export type CRMIdentityInput=z.infer<typeof crmIdentityCommand>;
