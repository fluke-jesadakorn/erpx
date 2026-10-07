import {z} from "zod";
import {pageLayoutSchema} from "@/lib/page-layouts";
export const pageLayoutInputSchema = z
  .object({
    action: z.enum(["save", "reset", "publish"]),
    expectedVersion: z.number().int().nonnegative(),
    layout: pageLayoutSchema.nullable(),
  })
  .strict();
export type PageLayoutInput=z.infer<typeof pageLayoutInputSchema>;
