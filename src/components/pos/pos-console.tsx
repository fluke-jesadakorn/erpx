"use client";
import dynamic from "next/dynamic";
import type {PosWorkspace} from "@/server/modules/pos/public/pos";
import type {PosInitialRead} from "@/lib/pos-read-identity";
const Front = dynamic(() => import("./pos-front-console").then(module => module.FrontPosConsole));
const Backoffice = dynamic(() => import("./pos-backoffice-console").then(module => module.BackofficePosConsole));
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
