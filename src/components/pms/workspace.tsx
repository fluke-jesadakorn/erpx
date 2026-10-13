"use client";
import dynamic from "next/dynamic";
import type {PmsWorkspace} from "@/server/modules/pms/public/pms";
const Front = dynamic(() => import("./pms-front-workspace").then(module => module.FrontPmsWorkspace));
const Backoffice = dynamic(() => import("./pms-backoffice-workspace").then(module => module.BackofficePmsWorkspace));
export function PmsWorkspaceView(props: {
  initial: PmsWorkspace;
  base: string;
  erpBase?: string;
  section: string;
  selectedGuestAccount?: string;
  crossLinks: { inventory: boolean; finance: boolean; pos: boolean };
  frontMode?: boolean;
  frontHref?: string;
  frontPosHref?: string;
  initialQueue?: string;
  selectedRecord?: string;
}) {
  const scopeIdentity = JSON.stringify([props.initial.userId, props.initial.snapshot?.property.id ?? null]);
  return props.frontMode ? <Front key={scopeIdentity} {...props}/> : <Backoffice key={scopeIdentity} {...props}/>;
}
