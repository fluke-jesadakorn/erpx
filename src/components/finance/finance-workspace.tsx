import { FinanceChrome } from "@/components/records/loaders/finance--finance-chrome";
import type { FinanceChromeProps } from "./finance-chrome";
import styles from "./finance-workspace.module.css";
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import { financePurchaseControls } from "@/server/modules/finance/public/purchases";

export async function FinanceWorkspace({ actor, ...props }: FinanceChromeProps & { actor?: WorkContext }) {
  const purchases = actor ? await financePurchaseControls(actor) : undefined;
  return <FinanceChrome {...props} purchases={purchases}/>;
}

export const financeStyles = styles;
