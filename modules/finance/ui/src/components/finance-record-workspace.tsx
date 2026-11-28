"use client";

import { useEmbeddedRecord } from "@erpx/platform-ui/components/record-context";
import { financeNavigation } from "@erpx/finance-ui/lib/finance-navigation";
import { useFeatureNavigation } from "@erpx/platform-shared-ui/components/features/feature-environment";
import { FinanceChrome, type FinanceChromeProps } from "@erpx/finance-ui/components/finance-chrome";
import { FinancePurchaseProvider } from "@erpx/finance-ui/components/purchase-context";

/** Detail pages keep their record widgets and authority; only their chrome changes. */
export function FinanceRecordWorkspace(props: FinanceChromeProps) {
  const context = useFeatureNavigation(), embedded = useEmbeddedRecord();
  return !embedded && context.financeWorkspaceV2 && financeNavigation(context).length > 0 ? <FinanceChrome {...props}/> : <FinancePurchaseProvider purchases={props.purchases} sourceHistory={props.sourceHistory}>{props.children}</FinancePurchaseProvider>;
}

export { FinanceLegacyHeading } from "@erpx/finance-ui/components/finance-legacy-heading";
