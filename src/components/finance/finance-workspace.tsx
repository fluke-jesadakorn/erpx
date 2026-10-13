import { FinanceChrome } from "@/components/records/loaders/finance--finance-chrome";
import type { FinanceChromeProps } from "./finance-chrome";
import styles from "./finance-workspace.module.css";

export function FinanceWorkspace(props: FinanceChromeProps) {
  return <FinanceChrome {...props}/>;
}

export const financeStyles = styles;
