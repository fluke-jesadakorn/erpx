"use client";

import { createContext, useContext, type ReactNode } from "react";

export type AttachmentFeatureMode = "enabled" | "read-only" | "disabled";

// Fail closed if a file surface is ever rendered outside the application-level
// provider.
const AttachmentFeatureContext =
  createContext<AttachmentFeatureMode>("disabled");

export function AttachmentFeatureProvider({
  mode,
  children,
}: {
  mode: AttachmentFeatureMode;
  children: ReactNode;
}) {
  return (
    <AttachmentFeatureContext.Provider value={mode}>
      {children}
    </AttachmentFeatureContext.Provider>
  );
}

export function useAttachmentFeatureMode() {
  return useContext(AttachmentFeatureContext);
}

export function canMutateAttachments(
  mode: AttachmentFeatureMode,
  authorized: boolean,
) {
  return mode === "enabled" && authorized;
}

export function AttachmentFeatureSurface({
  children,
}: {
  children: ReactNode;
}) {
  return useAttachmentFeatureMode() === "disabled" ? null : children;
}
