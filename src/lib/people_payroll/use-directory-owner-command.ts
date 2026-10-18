"use client";
import {useEffect, useLayoutEffect, useRef, useState} from "react";
import {useRouter} from "@/lib/shared/destination-router";
import {contextIsSwitching, registerUnsavedWork} from "@/lib/work_projection/context-switch";
import {createDirectoryOwnerCommand, type DirectoryCommandCurrent} from "./directory-owner-command";
import type {WorkCommandCompletion} from "@/lib/work_projection/work-fetch";

/** Optional callers remain inert; each enabled mounted owner binds its latest
 * actual SSR source in a layout effect, never a freshly acquired role. */
export function useDirectoryOwnerCommand(family: "directory" | "lifecycle", current: DirectoryCommandCurrent | null) {
  const router = useRouter(), [commands] = useState(() => createDirectoryOwnerCommand(family));
  const live = useRef<DirectoryCommandCurrent | null>(null), completion = useRef<((data: unknown) => void) | undefined>(undefined);
  useLayoutEffect(() => {live.current = current; commands.bind(() => live.current); return () => {live.current = null; commands.bind(() => null);};}, [current, commands]);
  const [busy, setBusy] = useState(false), [error, setError] = useState(""), [pending, setPending] = useState(false), [notice, setNotice] = useState("");
  const enabled = current !== null;
  useEffect(() => {
    if (!enabled) return;
    const dirty = () => commands.hasPending() || commands.busy(), unregister = registerUnsavedWork(dirty);
    const unload = (event: BeforeUnloadEvent) => {if (dirty() && !contextIsSwitching()) event.preventDefault();};
    window.addEventListener("beforeunload", unload);
    return () => {unregister(); window.removeEventListener("beforeunload", unload);};
  }, [commands, enabled]);
  async function apply<T>(run: () => ReturnType<typeof commands.retry>, preview: boolean): Promise<T | undefined> {
    setBusy(true); setError(""); setNotice("");
    try {
      const result = await run();
      result.assertCurrent();
      const unchanged = result.unchanged();
      if (!result.acknowledge()) throw Error("The original People receipt was already handled. Review the current owner.");
      if (!unchanged) {
        result.assertCurrent();
        if (!preview) router.refresh();
        completion.current = undefined;
        setNotice("The submitted version was saved. Your newer draft is retained."); return;
      }
      result.assertCurrent(); completion.current?.(result.data); completion.current = undefined;
      if (!preview) router.refresh();
      return result.data as T;
    } catch (cause) {setError(cause instanceof Error ? cause.message : "Unable to save People changes.");}
    finally {setBusy(false); setPending(commands.hasPending());}
  }
  function send<T>(path: string, input: unknown, form?: HTMLFormElement, onCompleted?: (data: T) => void): Promise<T | undefined> {
    if (commands.hasPending() || commands.busy()) {setError("Recover the original People request first. Your newer draft is retained."); return Promise.resolve(undefined);}
    completion.current = onCompleted ? value => onCompleted(value as T) : undefined;
    const submitted: WorkCommandCompletion | undefined = form ? {form, formEditVersion: form.getAttribute("data-erpx-edit-version")} : undefined;
    return apply<T>(() => commands.send(path, input, submitted), path === "placements/preview");
  }
  return {send, retry: () => apply(commands.retry, false), busy, error, notice, pending};
}
