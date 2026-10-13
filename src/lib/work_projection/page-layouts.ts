/** Validation entry retains the existing layout contract and presentation exports. */
import { z } from "zod";
export * from "@/lib/work_projection/page-layouts-view";

export const layoutItemSchema = z
  .object({
    id: z.string().min(1).max(160),
    columns: z.number().int().min(3).max(12),
    height: z.number().int().min(160).max(1600).nullable(),
    hidden: z.boolean(),
  })
  .strict();
export const pageLayoutSchema = z
  .object({
    schemaVersion: z.literal(1),
    widgets: z
      .array(layoutItemSchema)
      .max(120)
      .refine(
        (items) => new Set(items.map((w) => w.id)).size === items.length,
        "Choose each widget once.",
      ),
  })
  .strict();
