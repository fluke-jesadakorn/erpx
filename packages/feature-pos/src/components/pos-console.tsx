"use client";
import dynamic from "next/dynamic";
import type {PosWorkspace} from "@erpx/server-pos/public/pos";
import type {PosInitialRead} from "@erpx/feature-pos/lib/pos-read-identity";
const Front = dynamic(() => import("@erpx/feature-pos/components/pos-front-console").then(module => module.FrontPosConsole));
const Backoffice = dynamic(() => import("@erpx/feature-pos/components/pos-backoffice-console").then(module => module.BackofficePosConsole));
export function PosConsole(props: {
  initial: PosWorkspace;
  base: string;
  erpBase?: string;
  initialTab?: string;
  frontMode?: boolean;
  commandScopeKey?: string;
  frontHref?: string;
  recordOnly?: boolean;
  financeMode?: boolean;
  initialRead?: PosInitialRead;
}) {
  return props.frontMode && !props.recordOnly ? <Front {...props}/> : <Backoffice {...props}/>;
}
