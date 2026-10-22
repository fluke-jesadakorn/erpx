"use client";
import dynamic from "next/dynamic";
import { FrontLocalizedTree } from "@erpx/front/components/front-language";
import type {BackofficePmsWorkspaceSettingsScreenContext as BackofficePmsWorkspaceContext} from "@erpx/feature-pms/components/backoffice-screen-context";


const PmsSettingsPanel=dynamic(()=>import("@erpx/feature-pms/components/settings-panel").then(module=>module.PmsSettingsPanel));
export function BackofficePmsWorkspaceSettingsScreen({context}: {context: BackofficePmsWorkspaceContext & {p: NonNullable<BackofficePmsWorkspaceContext["p"]>}}) {
  const {can, erpBase, s, send, data, p, base, rows, tr, form, today, roomTypes, rateFields, money} = context;
  return <FrontLocalizedTree>{<PmsSettingsPanel can={can} erpBase={erpBase} s={s!} send={send} data={data} p={p!} base={base} rows={rows} tr={tr} form={form} today={today} roomTypes={roomTypes} rateFields={rateFields} money={money}/>}</FrontLocalizedTree>;
}
