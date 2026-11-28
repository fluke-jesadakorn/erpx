"use client";

import { useEffect } from "react";
import { readAppsMemory, writeAppsMemory } from "@erpx/work-projection-ui/lib/apps-launcher";

/** Called only after the server has authorized this Apps workspace. */
export function AppsWorkspaceMemory({ accountId, companyId, workspaceId }: {
  accountId: string; companyId: string; workspaceId: string;
}) {
  useEffect(() => {
    try {
      const memory = readAppsMemory(window.localStorage, accountId);
      writeAppsMemory(window.localStorage, accountId, { ...memory, companyId, workspaceId });
    } catch { /* Browser memory is optional. */ }
  }, [accountId, companyId, workspaceId]);
  return null;
}
