"use client";
import { createContext, useContext, type ReactNode } from "react";
export const EmbeddedRecord = createContext(false);
export const RecordScope = createContext("");
export function useEmbeddedRecord() { return useContext(EmbeddedRecord); }
export function RecordScopeProvider({ base, children }: { base: string; children: ReactNode }) {
  return <RecordScope value={base}>{children}</RecordScope>;
}
