"use client";
import { useEmbeddedRecord } from "@erpx/ui/components/record-context";
import { financeNavigation } from "@erpx/feature-finance/lib/finance-navigation";
import type { ReactNode } from "react";
import { useFeatureNavigation } from "@erpx/feature-shared/components/features/feature-environment";

export function FinanceLegacyHeading({ children }: { children: ReactNode }) {
  const context = useFeatureNavigation(), embedded = useEmbeddedRecord();
  return !embedded && context.financeWorkspaceV2 && financeNavigation(context).length > 0 ? null : <>{children}</>;
}
