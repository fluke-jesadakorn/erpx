"use client";
import { useEmbeddedRecord } from "@erpx/platform-ui/components/record-context";
import { financeNavigation } from "@erpx/finance-ui/lib/finance-navigation";
import type { ReactNode } from "react";
import { useFeatureNavigation } from "@erpx/platform-shared-ui/components/features/feature-environment";

export function FinanceLegacyHeading({ children }: { children: ReactNode }) {
  const context = useFeatureNavigation(), embedded = useEmbeddedRecord();
  return !embedded && context.financeWorkspaceV2 && financeNavigation(context).length > 0 ? null : <>{children}</>;
}
