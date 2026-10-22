"use client";
import { useSyncExternalStore } from "react";

function subscribe(changed:()=>void) {
  const system=window.matchMedia("(prefers-color-scheme: dark)");
  window.addEventListener("erpx-appearance",changed);
  system.addEventListener("change",changed);
  return ()=>{window.removeEventListener("erpx-appearance",changed);system.removeEventListener("change",changed);};
}
function snapshot():"light"|"dark" {
  const preference=document.documentElement.getAttribute("data-theme");
  return preference==="dark"||(preference==="system"&&window.matchMedia("(prefers-color-scheme: dark)").matches)?"dark":"light";
}
export function useWorkspaceColorMode() {
  return useSyncExternalStore(subscribe,snapshot,()=>"light" as const);
}
