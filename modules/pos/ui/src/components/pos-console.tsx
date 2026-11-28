"use client";
import dynamic from "next/dynamic";
import type {PosWorkspace} from "@erpx/pos-server/public/pos";
import type {PosInitialRead} from "@erpx/pos-ui/lib/pos-read-identity";
const Front = dynamic(() => import("@erpx/pos-ui/components/pos-front-console").then(module => module.FrontPosConsole));
const Backoffice = dynamic(() => import("@erpx/pos-ui/components/pos-backoffice-console").then(module => module.BackofficePosConsole));
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
