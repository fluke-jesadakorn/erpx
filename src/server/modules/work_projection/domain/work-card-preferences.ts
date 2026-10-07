import {z} from "zod";
import {workCardIds} from "@/lib/work-cards";
export const workCardPreferenceSchema = z
  .object({
    pinnedIds: z
      .array(z.enum(workCardIds))
      .max(6)
      .refine(
        (ids) => new Set(ids).size === ids.length,
        "Choose each shortcut once.",
      )
      .nullable(),
    expectedVersion: z.number().int().nonnegative(),
  })
  .strict();
export type WorkCardPreferenceInput=z.infer<typeof workCardPreferenceSchema>;
