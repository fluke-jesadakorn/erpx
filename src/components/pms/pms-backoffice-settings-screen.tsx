"use client";
import dynamic from "next/dynamic";
import { FrontLocalizedTree } from "@/components/front/front-language";
import type {BackofficePmsWorkspaceSettingsScreenContext as BackofficePmsWorkspaceContext} from "./backoffice-screen-context";


const PmsSettingsPanel=dynamic(()=>import("./settings-panel").then(module=>module.PmsSettingsPanel));
export function BackofficePmsWorkspaceSettingsScreen({context}: {context: BackofficePmsWorkspaceContext & {p: NonNullable<BackofficePmsWorkspaceContext["p"]>}}) {
  const {can, erpBase, s, send, data, p, base, rows, tr, form, today, roomTypes, rateFields, money} = context;
  return <FrontLocalizedTree>{<PmsSettingsPanel can={can} erpBase={erpBase} s={s!} send={send} data={data} p={p!} base={base} rows={rows} tr={tr} form={form} today={today} roomTypes={roomTypes} rateFields={rateFields} money={money}/>}</FrontLocalizedTree>;
}
