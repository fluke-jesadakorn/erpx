"use client";
import { useEmbeddedRecord } from "@/components/records/record-context";
import { financeNavigation } from "@/lib/finance/finance-navigation";
import type { ReactNode } from "react";
import { useFeatureNavigation } from "@/components/shared/features/feature-environment";

export function FinanceLegacyHeading({ children }: { children: ReactNode }) {
  const context = useFeatureNavigation(), embedded = useEmbeddedRecord();
  return !embedded && context.financeWorkspaceV2 && financeNavigation(context).length > 0 ? null : <>{children}</>;
}
