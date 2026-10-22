"use client";

import { useEffect, useState, useSyncExternalStore } from "react";
import { useRouter } from "next/navigation";
import { Button, type UiAppearance } from "@erpx/ui/components/foundation";
import { appearancePreference } from "@erpx/feature-shared/lib/appearance-preference";
import { contextGeneration, contextIsSwitching, hasUnsavedWork } from "@erpx/feature-work_projection/lib/context-switch";
import { createAccountPreferencesClient } from "@erpx/feature-identity_access/lib/account-preferences-client";
import {createAccountLanguageRefresh,createAccountPreferenceRefresh} from '@erpx/feature-identity_access/lib/account-preference-refresh';
import type { AccountPreferences, AccountPreferenceSnapshot } from "@erpx/server-identity_access/public/account-preferences";

import { AccountPreferencesContext as Context } from "@erpx/feature-identity_access/components/account-preferences-context";
const valuesFromDocument = () => ({ language: document.documentElement.lang === "th" ? "th" as const : "en" as const,
  appearance: appearancePreference(document.documentElement.getAttribute("data-theme") ?? undefined) });

function writeCookie(name: string, value: string) {
  document.cookie = `${name}=${value}; Path=/; Max-Age=31536000; SameSite=Lax${location.protocol === "https:" ? "; Secure" : ""}`;
}
async function responseSnapshot(response: Response): Promise<AccountPreferenceSnapshot> {
  const body = await response.json().catch(()=>{throw Object.assign(new Error('Could not read your account preferences. Try again.'),{status:response.status});});
  if (!response.ok) throw Object.assign(new Error(body?.error?.message ?? "Could not save your preferences. Try again."),{status:response.status});
  const snapshot = body?.data as AccountPreferenceSnapshot;
  if (typeof snapshot?.key !== "string" || !snapshot.preference ||
    !["en", "th"].includes(snapshot.preference.language) || !["system", "light", "dark"].includes(snapshot.preference.appearance) ||
    !Number.isSafeInteger(snapshot.preference.version) || snapshot.preference.version < 0) throw new Error("Could not read your preferences. Try again.");
  return snapshot;
}

function model(snapshot: AccountPreferenceSnapshot, refresh: () => void) {
  const languageRefresh=createAccountLanguageRefresh(snapshot.preference?.language??'en',{active:()=>!contextIsSwitching()&&typeof document!=='undefined'&&document.visibilityState!=='hidden',dirty:hasUnsavedWork,refresh});
  const client = createAccountPreferencesClient(snapshot, {
    read: signal => fetch("/api/v1/account/preferences", { cache: "no-store", signal }).then(responseSnapshot),
    write: (accountKey, expectedVersion, values, signal) => fetch("/api/v1/account/preferences", {
      method: "PATCH", headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ accountKey, expectedVersion, ...values }), signal,
    }).then(responseSnapshot),
    generation: () => contextIsSwitching() ? null : contextGeneration(),
    apply: (preference: AccountPreferences) => {
      document.documentElement.lang = preference.language;
      document.documentElement.setAttribute("data-theme", preference.appearance);
      writeCookie("erpx_language", preference.language);
      writeCookie("erpx_appearance", preference.appearance);
      window.dispatchEvent(new CustomEvent("erpx-appearance", { detail: { source: "account-sync" } }));
      // Front's context updates immediately. Server labels refresh only when
      // that cannot disturb an unrelated editor's unsaved work.
      languageRefresh.apply(preference.language);
    },
    changed: () => {
      if (typeof BroadcastChannel === "undefined") return;
      const channel = new BroadcastChannel("erpx-account-preferences");
      channel.postMessage({ key: snapshot.key });
      channel.close();
    },
  });
  return { key: snapshot.key, client,languageRefresh };
}

export function AccountPreferencesProvider({ snapshot, children, appearance = "standard" }: {
  snapshot: AccountPreferenceSnapshot; children: React.ReactNode; appearance?: UiAppearance;
}) {
  const router = useRouter();
  const [holder, setHolder] = useState(() => model(snapshot, () => router.refresh()));
  // Replace only this account's preference controller; children keep their
  // identity. A context switch already owns the business draft guard.
  if (holder.key !== snapshot.key) setHolder(model(snapshot, () => router.refresh()));
  const { client,languageRefresh } = holder;
  const state = useSyncExternalStore(client.subscribe, client.getSnapshot, client.getSnapshot);
  useEffect(() => {if(snapshot.preference)languageRefresh.observe(snapshot.preference.language);client.observe(snapshot);const current=client.getSnapshot().preference;if(current)languageRefresh.apply(current.language);}, [client,languageRefresh,snapshot]);
  useEffect(() => {
    let away = false;
    let cleanTimer:number|null=null;
    const synchronization=createAccountPreferenceRefresh({visible:()=>document.visibilityState!=='hidden',active:()=>!contextIsSwitching()&&/^[a-f0-9]{64}$/.test(holder.key),read:()=>{const values=valuesFromDocument(),before=client.getSnapshot();if(!before.preference||before.preference.version===0&&before.revision===0){languageRefresh.observe(values.language);languageRefresh.apply(values.language);}client.seedLegacy(values);return client.refresh();},setTimer:(run,delay)=>window.setTimeout(run,delay),clearTimer:id=>window.clearTimeout(id)});
    synchronization.start();
    const channel = typeof BroadcastChannel === "undefined" ? null : new BroadcastChannel("erpx-account-preferences");
    const blur = () => { away = true; };
    const focus = () => {
      if (!away || document.visibilityState === "hidden") return;
      away = false;
      void synchronization.focus();languageRefresh.flush();
    };
    const visibility = () => { if (document.visibilityState === "hidden") {blur();synchronization.hidden();} else focus(); };
    const peer = (event: MessageEvent) => { if (event.data?.key === holder.key) void synchronization.peer(); };
    const clean=()=>{if(!languageRefresh.pending()||cleanTimer!==null)return;cleanTimer=window.setTimeout(()=>{cleanTimer=null;languageRefresh.flush();},0);};
    const appearanceChanged = (event: Event) => {
      if ((event as CustomEvent).detail?.source === "account-sync" || contextIsSwitching()) return;
      const values = valuesFromDocument();
      client.seedLegacy({ ...values, appearance: client.getSnapshot().preference?.appearance ?? values.appearance });
      client.change({ appearance: values.appearance }, values);
    };
    const languageChanged = (event: Event) => {
      const detail = (event as CustomEvent).detail;
      if (!["en", "th"].includes(detail?.language) || contextIsSwitching()) return;
      const values = valuesFromDocument();
      client.seedLegacy({ ...values, language: detail.previousLanguage === "th" ? "th" : "en" });
      client.change({ language: detail.language }, values);
    };
    const saved = (event: Event) => {
      const detail = (event as CustomEvent).detail as AccountPreferenceSnapshot | undefined;
      if (detail?.key === holder.key) client.receive(detail);
    };
    const cancel = () => {synchronization.cancel();languageRefresh.cancel();client.cancel();};
    const contextRestored=()=>{away=true;focus();};
    channel?.addEventListener("message", peer);
    window.addEventListener("blur", blur); window.addEventListener("focus", focus);
    document.addEventListener("visibilitychange", visibility);
    window.addEventListener("erpx-appearance", appearanceChanged);
    window.addEventListener("erpx:account-language", languageChanged);
    window.addEventListener("erpx:account-preferences-saved", saved);
    window.addEventListener("erpx:context-switch-started", cancel);
    window.addEventListener('erpx:context-switch-finished',contextRestored);
    window.addEventListener('erpx:record-command-saved',clean);window.addEventListener('erpx:record-saved',clean);window.addEventListener('erpx:work-changed',clean);document.addEventListener('click',clean,true);document.addEventListener('change',clean,true);
    return () => {
      synchronization.dispose();if(cleanTimer!==null)window.clearTimeout(cleanTimer);client.cancel();channel?.close();
      window.removeEventListener("blur", blur); window.removeEventListener("focus", focus);
      document.removeEventListener("visibilitychange", visibility);
      window.removeEventListener("erpx-appearance", appearanceChanged);
      window.removeEventListener("erpx:account-language", languageChanged);
      window.removeEventListener("erpx:account-preferences-saved", saved);
      window.removeEventListener("erpx:context-switch-started", cancel);
      window.removeEventListener('erpx:context-switch-finished',contextRestored);
      window.removeEventListener('erpx:record-command-saved',clean);window.removeEventListener('erpx:record-saved',clean);window.removeEventListener('erpx:work-changed',clean);document.removeEventListener('click',clean,true);document.removeEventListener('change',clean,true);
    };
  }, [client,languageRefresh,holder.key]);
  return <Context.Provider value={state}>{children}{state.error && <div data-ui-appearance={appearance} role="alert" className="content">
    {state.error}{" "}<Button appearance={appearance} type="button" variant="plain" onClick={() => client.retry()}>Try again</Button>
  </div>}</Context.Provider>;
}

