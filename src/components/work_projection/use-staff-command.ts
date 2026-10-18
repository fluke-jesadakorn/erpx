"use client";
import { useRef, useState, useTransition } from "react";
import { canonicalUiHref } from "@/lib/work_projection/destination-registry";
import { workFetch } from "@/lib/work_projection/work-fetch";
import { useRouter } from "@/lib/shared/destination-router";

export function useStaffCommand(base: string) {
  const router = useRouter(),
    [refreshing, startRefresh] = useTransition(),
    attempt = useRef<{ signature: string; key: string } | null>(null),
    [busy, setBusy] = useState(false),
    [error, setError] = useState("");
  async function send(path: string, input: unknown, destination?: string) {
    const body = JSON.stringify(input),
      signature = path + body;
    if (attempt.current?.signature !== signature)
      attempt.current = { signature, key: crypto.randomUUID() };
    setBusy(true);
    setError("");
    try {
      const response = await workFetch(`/api/v1${base}/${path}`, {
        method: "POST",
        headers: {
          "Content-Type": "application/json",
          "Idempotency-Key": attempt.current.key,
        },
        body,
      });
      const data = (await response.json()) as {
        data: { id: string };
        error?: { message: string };
      };
      if (!response.ok)
        throw new Error(data.error?.message ?? "Unable to save. Your details are still here.");
      attempt.current = null;
      startRefresh(() => {
        if (destination) router.push(canonicalUiHref(`${base}/${destination}/${data.data.id}`));
        else router.refresh();
      });
      return data.data;
    } catch (e) {
      setError(
        e instanceof Error ? e.message : "Connection interrupted. Retry to finish the same action.",
      );
      return null;
    } finally {
      setBusy(false);
    }
  }
  return { send, busy: busy || refreshing, error };
}
