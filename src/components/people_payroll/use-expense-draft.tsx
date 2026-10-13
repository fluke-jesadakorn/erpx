"use client";
import { useRecoverableDraft } from "@/components/ui/use-recoverable-draft";
export function useExpenseDraft<T extends Record<string, string>>(
  key: string,
  initial: T,
) {
  return useRecoverableDraft(key, initial, "claim");
}
