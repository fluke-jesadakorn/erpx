"use client";
import {useEffect, useLayoutEffect, useRef, useState, type FormEvent} from "react";
import {useRouter} from "@/lib/shared/destination-router";
import {contextIsSwitching, registerUnsavedWork} from "@/lib/work_projection/context-switch";
import {createOrganizationAccessCommands, type OrganizationAccessCommandScope} from "./organization-access-command";
export function useOrganizationAccessCommand(scope: Omit<OrganizationAccessCommandScope, "draft">, store?: ReturnType<typeof createOrganizationAccessCommands>) {
  const live = useRef<typeof scope | null>(null), draft = useRef(0), router = useRouter();
  const [commands] = useState(() => store ?? createOrganizationAccessCommands(() => null));
  const [busy, setBusy] = useState(false), [pending, setPending] = useState(commands.hasPending), [dirty, setDirty] = useState(false), [notice, setNotice] = useState("");
  const dirtyRef = useRef(false), completed = useRef<Awaited<ReturnType<typeof commands.send>> | null>(null);
  useLayoutEffect(() => {live.current = scope; commands.bind(() => live.current ? {...live.current, draft: draft.current} : null); return () => {live.current = null; commands.bind(() => null);};}, [scope, commands]);
  useEffect(() => {
    const unsaved = () => dirtyRef.current || commands.hasPending() || commands.busy(), unregister = registerUnsavedWork(unsaved);
    const unload = (event: BeforeUnloadEvent) => {if (unsaved() && !contextIsSwitching()) event.preventDefault();};
    window.addEventListener("beforeunload", unload); return () => {unregister(); window.removeEventListener("beforeunload", unload);};
  }, [commands]);
  function markEdited(event: FormEvent<HTMLElement>) {const target = event.target as HTMLElement; if (target.closest("[data-organization-owner-external]") || target.closest("[data-organization-command-owner]") !== event.currentTarget || !target.closest("form,[data-organization-command-draft]")) return; draft.current++; dirtyRef.current = true; setDirty(true); target.closest("form")?.setAttribute("data-erpx-edit-version", String(draft.current));}
  async function run<T>(work: () => ReturnType<typeof commands.retry>): Promise<T> {
    setBusy(true); setNotice(""); completed.current = null;
    try {
      const result = await work(); result.assertCurrent();
      if (!result.acknowledge()) throw Error("The original Organization receipt was already handled.");
      completed.current = result;
      if (!result.path.endsWith("/preview") && !result.path.endsWith("/remove-preview")) {result.assertCurrent(); if (result.unchanged()) {dirtyRef.current = false; setDirty(false);} setNotice("The original Organization action is confirmed. Review its current owner history."); router.refresh();}
      if (!result.unchanged()) {setNotice("The submitted Organization action is confirmed. Your newer draft is retained."); throw Error("The submitted Organization action is confirmed. Your newer draft is retained.");}
      return result.data as T;
    } finally {setBusy(false); setPending(commands.hasPending());}
  }
  function assertCompleted() {const result = completed.current; if (!result) throw Error("Review the original Organization receipt."); result.assertCurrent(); if (!result.unchanged()) throw Error("The submitted Organization action is confirmed. Your newer draft is retained.");}
  function discard() {if (commands.hasPending() || commands.busy()) return false; completed.current = null; dirtyRef.current = false; setDirty(false); draft.current++; return true;}
  return {busy, pending, dirty, notice, markEdited, assertCompleted, discard,
    send: <T,>(path: string, input: unknown, form?: HTMLFormElement) => {const selected = form ?? document.activeElement?.closest("form") ?? undefined; return run<T>(() => commands.send(path, input, selected ? {form: selected, formEditVersion: selected.getAttribute("data-erpx-edit-version")} : undefined));},
    retry: () => run(commands.retry),
  };
}
