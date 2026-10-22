"use client";

import { createContext, useContext } from "react";

export type SwitchRequest = {
  action: () => Promise<void>;
  companyId?: string;
  from?: string;
  returnTo?: string;
  onCommitted?: () => void;
  scroll?: { x: number; y: number };
  inspect?: boolean;
};
type SwitchContext = { busy: boolean; switchContext: (request: SwitchRequest) => Promise<boolean> };
export const ContextSwitchContext = createContext<SwitchContext | null>(null);

export function useContextSwitch() {
  const value = useContext(ContextSwitchContext);
  if (!value) throw new Error("ContextSwitchProvider is required.");
  return value;
}
