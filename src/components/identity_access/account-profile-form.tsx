"use client";

import { useEffect, useLayoutEffect, useRef, useState, type FormEvent } from "react";
import { useRouter } from "next/navigation";
import type { AccountProfile } from "@/server/modules/identity_access/public/account-preferences";
import { contextGeneration, contextIsSwitching, registerUnsavedWork } from "@/lib/work_projection/context-switch";
import { Button, Field, InlineNotice, Select, Stack, TextInput } from "@/components/ui/foundation";
import { useHydrated } from "@/components/ui/use-hydrated";
import {useAccountPreferences} from './account-preferences-context';
import {accountProfileAcknowledgement,accountProfilePreferenceEvent,accountProfileResponseCurrent,observeAccountProfileWitness,type AccountProfileWitness} from '@/lib/identity_access/account-profile-client';

const draftOf = (profile: AccountProfile) => ({ name: profile.name, language: profile.preferences.language, appearance: profile.preferences.appearance });
const unchanged = (draft: ReturnType<typeof draftOf>, baseline: AccountProfile) => JSON.stringify(draft) === JSON.stringify(draftOf(baseline));

export function AccountProfileForm({ accountKey, initial, language }: { accountKey: string; initial: AccountProfile; language: "en" | "th" }) {
  const router = useRouter(), th = language === "th";
  const hydrated = useHydrated();
  const sharedPreferences=useAccountPreferences();
  const witness=useRef<AccountProfileWitness>({accountKey,version:initial.preferences.version,foreignAccount:false,pending:false}),edits=useRef(0);
  useLayoutEffect(()=>{witness.current=observeAccountProfileWitness(witness.current,accountKey,initial.preferences.version,sharedPreferences);},[accountKey,initial,sharedPreferences]);
  useEffect(()=>{
    const observed=(event:Event)=>{const peer=accountProfilePreferenceEvent((event as CustomEvent<unknown>).detail);if(peer?.key===accountKey)witness.current=observeAccountProfileWitness(witness.current,accountKey,0,peer);};
    window.addEventListener('erpx:account-preferences-saved',observed);return()=>window.removeEventListener('erpx:account-preferences-saved',observed);
  },[accountKey]);
  const [state, setState] = useState(() => ({ received: initial, baseline: initial, draft: draftOf(initial) }));
  const [pending, setPending] = useState(false), [error, setError] = useState(""), [saved, setSaved] = useState(false), [invalidated, setInvalidated] = useState(false);
  // Adjust only when a different server seed arrives. A dirty draft retains
  // its original values and optimistic witness through a refresh.
  if (state.received !== initial) setState(unchanged(state.draft, state.baseline)&&initial.preferences.version>=state.baseline.preferences.version
    ? { received: initial, baseline: initial, draft: draftOf(initial) } : { ...state, received: initial });
  const dirty = !unchanged(state.draft, state.baseline), dirtyRef = useRef(dirty);
  const controller = useRef<AbortController | null>(null), epoch = useRef(0);
  useEffect(() => { dirtyRef.current = dirty; }, [dirty]);
  useEffect(() => {
    const operationEpoch = epoch;
    const unregister = registerUnsavedWork(() => dirtyRef.current);
    const unload = (event: BeforeUnloadEvent) => { if (dirtyRef.current && !contextIsSwitching()) event.preventDefault(); };
    const changed = () => { epoch.current++; controller.current?.abort(); controller.current = null; setPending(false); setInvalidated(true); };
    window.addEventListener("beforeunload", unload);
    window.addEventListener("erpx:work-context-changed", changed);
    window.addEventListener("erpx:context-switch-started", changed);
    return () => {
      operationEpoch.current++; controller.current?.abort(); unregister();
      window.removeEventListener("beforeunload", unload);
      window.removeEventListener("erpx:work-context-changed", changed);
      window.removeEventListener("erpx:context-switch-started", changed);
    };
  }, []);
  function edit(patch: Partial<ReturnType<typeof draftOf>>) {
    edits.current++;
    setState(current => ({ ...current, draft: { ...current.draft, ...patch } })); setSaved(false); setError("");
  }
  async function loadSaved() {
    if (controller.current || invalidated || dirty && !window.confirm(th ? "ทิ้งการแก้ไขที่ยังไม่บันทึกและโหลดค่าปัจจุบัน?" : "Discard unsaved account changes and load the saved settings?")) return;
    const abort = new AbortController(), attempt = ++epoch.current, generation = contextGeneration();
    const capturedRevision=edits.current;
    controller.current = abort; setPending(true); setError(""); setSaved(false);
    try {
      const response = await fetch(`/api/v1/account/profile?${new URLSearchParams({ accountKey })}`, { cache: "no-store", signal: abort.signal });
      const body = await response.json();
      if (attempt !== epoch.current || generation !== contextGeneration() || abort.signal.aborted) return;
      if (!response.ok) throw new Error(body.error?.message ?? "Unable to load your saved settings.");
      const next = accountProfileAcknowledgement(body);
      if(!accountProfileResponseCurrent(witness.current,accountKey,next))throw Error(th?'การตั้งค่าบัญชีเปลี่ยนแล้ว โปรดโหลดและตรวจสอบค่าที่บันทึกไว้':'Your account settings changed elsewhere. Load and review the saved settings.');
      witness.current=observeAccountProfileWitness(witness.current,accountKey,next.preferences.version,null);
      setState(current => ({ ...current, baseline: next, draft: edits.current===capturedRevision?draftOf(next):current.draft }));
      window.dispatchEvent(new CustomEvent("erpx:account-preferences-saved", { detail: { key: accountKey, preference: next.preferences } }));
      router.refresh();
    } catch (failure) {
      if (attempt === epoch.current && !abort.signal.aborted) setError(failure instanceof Error ? failure.message : "Unable to load your saved settings.");
    } finally {
      if (controller.current === abort) controller.current = null;
      if (attempt === epoch.current) setPending(false);
    }
  }
  async function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!hydrated || controller.current || invalidated || !event.currentTarget.reportValidity()) return;
    const abort = new AbortController(), attempt = ++epoch.current, generation = contextGeneration();
    const submitted={draft:{...state.draft},baseline:state.baseline,revision:edits.current};
    controller.current = abort; setPending(true); setError(""); setSaved(false);
    try {
      const response = await fetch("/api/v1/account/profile", { method: "PATCH", headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ ...submitted.draft, accountKey, expectedName: submitted.baseline.name, expectedVersion: submitted.baseline.preferences.version }), signal: abort.signal });
      const result = await response.json();
      if (attempt !== epoch.current || generation !== contextGeneration() || abort.signal.aborted) return;
      if (!response.ok) throw new Error(result.error?.message ?? (th ? "บันทึกไม่สำเร็จ" : "Unable to save your account settings."));
      const next = accountProfileAcknowledgement(result);
      if(next.preferences.version!==submitted.baseline.preferences.version+1)throw Error('Could not confirm the saved account settings. Load saved settings before trying again.');
      if(!accountProfileResponseCurrent(witness.current,accountKey,next))throw Error(th?'การตั้งค่าบัญชีเปลี่ยนแล้ว โปรดโหลดและตรวจสอบค่าที่บันทึกไว้':'Your account settings changed elsewhere. Load and review the saved settings.');
      witness.current=observeAccountProfileWitness(witness.current,accountKey,next.preferences.version,null);
      setState(current => ({ ...current, baseline: next, draft: edits.current===submitted.revision?draftOf(next):current.draft })); setSaved(edits.current===submitted.revision);
      // The existing owner controller rejects a stale preference event using
      // its live version, including before a new React context commit.
      if(sharedPreferences?.key!==accountKey){
        document.documentElement.setAttribute("data-theme", next.preferences.appearance);
        document.cookie = `erpx_appearance=${next.preferences.appearance}; Path=/; Max-Age=31536000; SameSite=Lax; Secure`;
        document.cookie = `erpx_language=${next.preferences.language}; Path=/; Max-Age=31536000; SameSite=Lax; Secure`;
        window.dispatchEvent(new CustomEvent("erpx-appearance", { detail: { source: "account-sync" } }));
      }
      window.dispatchEvent(new CustomEvent("erpx:account-preferences-saved", { detail: { key: accountKey, preference: next.preferences } }));
      router.refresh();
    } catch (failure) {
      if (attempt === epoch.current && !abort.signal.aborted) setError(failure instanceof Error ? failure.message : "Unable to save your account settings.");
    } finally {
      if (controller.current === abort) controller.current = null;
      if (attempt === epoch.current) setPending(false);
    }
  }
  return <form onSubmit={submit} data-work-dirty={dirty ? "true" : undefined}><Stack>
    <Field appearance="standard" label={th ? "ชื่อที่แสดง" : "Display name"} required>
      <TextInput appearance="standard" autoComplete="name" required maxLength={160} value={state.draft.name} disabled={!hydrated || pending || invalidated} onChange={event => edit({ name: event.target.value })} />
    </Field>
    <Field appearance="standard" label={th ? "อีเมล" : "Email"}><TextInput appearance="standard" type="email" autoComplete="email" readOnly value={state.baseline.email} /></Field>
    <Field appearance="standard" label={th ? "ภาษา" : "Language"}>
      <Select appearance="standard" value={state.draft.language} disabled={!hydrated || pending || invalidated} onChange={event => edit({ language: event.target.value as "en" | "th" })}>
        <option value="en">English</option><option value="th">ไทย</option>
      </Select>
    </Field>
    <Field appearance="standard" label={th ? "รูปแบบสี" : "Theme"}>
      <Select appearance="standard" value={state.draft.appearance} disabled={!hydrated || pending || invalidated} onChange={event => edit({ appearance: event.target.value as "system" | "light" | "dark" })}>
        <option value="system">{th ? "ตามอุปกรณ์" : "System"}</option><option value="light">{th ? "สว่าง" : "Light"}</option><option value="dark">{th ? "มืด" : "Dark"}</option>
      </Select>
    </Field>
    {error && <InlineNotice tone="danger" role="alert">{error}{" "}<Button appearance="standard" variant="plain" type="button" disabled={pending || invalidated} onClick={() => void loadSaved()}>{th ? "โหลดค่าที่บันทึกไว้" : "Load saved settings"}</Button></InlineNotice>}
    {saved && <InlineNotice tone="success">{th ? "บันทึกการตั้งค่าบัญชีแล้ว" : "Account settings saved."}</InlineNotice>}
    {invalidated && <InlineNotice tone="warning">{th ? "บัญชีหรือเซสชันเปลี่ยน โปรดโหลดหน้าใหม่ก่อนทำต่อ" : "Your account or session changed. Reload before continuing."}</InlineNotice>}
    <div><Button appearance="standard" type="submit" clientManaged pending={pending} disabled={invalidated || !dirty}>{th ? "บันทึกการตั้งค่า" : "Save account settings"}</Button></div>
  </Stack></form>;
}
