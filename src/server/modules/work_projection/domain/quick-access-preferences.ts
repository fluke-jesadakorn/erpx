import {z} from "zod";
export const quickAccessPreferenceSchema = z.object({
  pinnedIds: z.array(z.string().min(1).max(200)).max(6)
    .refine((ids) => new Set(ids).size === ids.length, "Choose each shortcut once.")
    .nullable(),
  expectedVersion: z.number().int().nonnegative(),
}).strict();

export type SaveInput = z.infer<typeof quickAccessPreferenceSchema> & {
  userId: string;
  surface: string;
  contextKey: string;
  availableIds: string[];
  requestKey: string;
};

