import {z} from 'zod';
export const legacyStripeObjectSchema = z.object({
  id: z.string().optional(),
  url: z.string().optional(),
  livemode: z.boolean().optional(),
  data: z
    .array(
      z.object({
        id: z.string(),
        livemode: z.boolean(),
        created: z.number(),
        status: z.string(),
        current_period_end: z.number().nullable().optional(),
      }),
    )
    .optional(),
});

export type LegacyStripeObject=z.infer<typeof legacyStripeObjectSchema>;
export type LegacyStripeEvent={id:string;type:string;created:number;livemode?:boolean;data?:{object?:{customer?:unknown;livemode?:boolean}}};
export type LegacyStripeRequest=(path:string,fields?:Record<string,string>,key?:string)=>Promise<LegacyStripeObject>;
