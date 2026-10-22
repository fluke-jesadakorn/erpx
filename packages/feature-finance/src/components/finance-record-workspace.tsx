"use client";

import { useEmbeddedRecord } from "@erpx/ui/components/record-context";
import { financeNavigation } from "@erpx/feature-finance/lib/finance-navigation";
import { useFeatureNavigation } from "@erpx/feature-shared/components/features/feature-environment";
import { FinanceChrome, type FinanceChromeProps } from "@erpx/feature-finance/components/finance-chrome";
import { FinancePurchaseProvider } from "@erpx/feature-finance/components/purchase-context";

/** Detail pages keep their record widgets and authority; only their chrome changes. */
export function FinanceRecordWorkspace(props: FinanceChromeProps) {
  const context = useFeatureNavigation(), embedded = useEmbeddedRecord();
  return !embedded && context.financeWorkspaceV2 && financeNavigation(context).length > 0 ? <FinanceChrome {...props}/> : <FinancePurchaseProvider purchases={props.purchases} sourceHistory={props.sourceHistory}>{props.children}</FinancePurchaseProvider>;
}

export { FinanceLegacyHeading } from "@erpx/feature-finance/components/finance-legacy-heading";
