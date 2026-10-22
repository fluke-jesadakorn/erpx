import { z } from "zod";

export type AccountPreferences = { language: "en" | "th"; appearance: "system" | "light" | "dark"; version: number };
export type AccountProfile = { name: string; email: string; preferences: AccountPreferences };
export const defaultAccountPreferences: AccountPreferences = { language: "en", appearance: "system", version: 0 };
export const accountPreferencesInput = z.object({
  accountKey: z.string().regex(/^[a-f0-9]{64}$/),
  name: z.string().trim().min(1).max(160).refine(value => !/[\u0000-\u001f\u007f]/.test(value), "Choose a display name without control characters."),
  expectedName: z.string().max(160),
  language: z.enum(["en", "th"]),
  appearance: z.enum(["system", "light", "dark"]),
  expectedVersion: z.number().int().nonnegative(),
}).strict();
export type AccountPreferencesInput = z.infer<typeof accountPreferencesInput>;
export const accountPreferencePatchInput = z.object({
  accountKey: z.string().regex(/^[a-f0-9]{64}$/),
  expectedVersion: z.number().int().nonnegative(),
  language: z.enum(["en", "th"]).optional(),
  appearance: z.enum(["system", "light", "dark"]).optional(),
}).strict().refine(value => value.language !== undefined || value.appearance !== undefined);
export type AccountPreferenceSnapshot = { key: string; preference: AccountPreferences | undefined };
