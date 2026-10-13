"use client";
import { useRef, useState } from "react";
import { betaFetch } from "@/lib/shared/beta-client";
export function useSocialCommand() {
  const attempts = useRef(new Map<string, string>()),
    running = useRef(false);
  const [busy, setBusy] = useState(false),
    [message, setMessage] = useState("");
  async function send<T>(url: string, input?: unknown): Promise<T> {
    if (input === undefined) return betaFetch<T>(url);
    const fingerprint = JSON.stringify({ url, input }),
      key = attempts.current.get(fingerprint) ?? crypto.randomUUID();
    attempts.current.set(fingerprint, key);
    const result = await betaFetch<T>(url, input, key);
    attempts.current.delete(fingerprint);
    return result;
  }
  async function run(action: () => Promise<void>) {
    if (running.current) return;
    running.current = true;
    setBusy(true);
    setMessage("");
    try {
      await action();
    } catch (e) {
      setMessage(e instanceof Error ? e.message : "Please try again.");
    } finally {
      running.current = false;
      setBusy(false);
    }
  }
  return { run, send, busy, message, setMessage };
}
