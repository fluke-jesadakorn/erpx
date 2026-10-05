type StarterAccount = {
  code: string;
  name: string;
  category:
    | "asset"
    | "liability"
    | "equity"
    | "revenue"
    | "expense"
    | "other_income"
    | "other_expense";
  controlKind: "none" | "bank" | "receivable" | "payable";
};
/** Product starting configuration, not certification of an accounting standard or tax eligibility. */
export const thaiFinanceStarterAccounts: readonly StarterAccount[] = [
  {
    code: "1100",
    name: "Operating bank",
    category: "asset",
    controlKind: "bank",
  },
  {
    code: "1200",
    name: "Trade receivables",
    category: "asset",
    controlKind: "receivable",
  },
  {
    code: "1400",
    name: "Input VAT pending eligibility review",
    category: "asset",
    controlKind: "none",
  },
  {
    code: "1500",
    name: "Equipment cost",
    category: "asset",
    controlKind: "none",
  },
  {
    code: "1590",
    name: "Accumulated depreciation",
    category: "asset",
    controlKind: "none",
  },
  {
    code: "1600",
    name: "Right-of-use assets",
    category: "asset",
    controlKind: "none",
  },
  {
    code: "1690",
    name: "Accumulated right-of-use depreciation",
    category: "asset",
    controlKind: "none",
  },
  {
    code: "2100",
    name: "Trade payables",
    category: "liability",
    controlKind: "payable",
  },
  {
    code: "2200",
    name: "Output VAT",
    category: "liability",
    controlKind: "none",
  },
  {
    code: "2300",
    name: "Deferred service revenue",
    category: "liability",
    controlKind: "none",
  },
  {
    code: "2400",
    name: "Lease liabilities",
    category: "liability",
    controlKind: "none",
  },
  {
    code: "2500",
    name: "Loan principal",
    category: "liability",
    controlKind: "none",
  },
  {
    code: "3100",
    name: "Share capital",
    category: "equity",
    controlKind: "none",
  },
  {
    code: "3200",
    name: "Retained earnings",
    category: "equity",
    controlKind: "none",
  },
  {
    code: "3990",
    name: "Fiscal profit transfer",
    category: "equity",
    controlKind: "none",
  },
  {
    code: "4100",
    name: "Service revenue",
    category: "revenue",
    controlKind: "none",
  },
  {
    code: "5100",
    name: "Operating expenses",
    category: "expense",
    controlKind: "none",
  },
  {
    code: "5200",
    name: "Depreciation expense",
    category: "expense",
    controlKind: "none",
  },
  {
    code: "5300",
    name: "Interest expense",
    category: "expense",
    controlKind: "none",
  },
  {
    code: "7100",
    name: "Foreign exchange gains",
    category: "other_income",
    controlKind: "none",
  },
  {
    code: "7200",
    name: "Foreign exchange losses",
    category: "other_expense",
    controlKind: "none",
  },
  {
    code: "7300",
    name: "Asset disposal gains",
    category: "other_income",
    controlKind: "none",
  },
  {
    code: "7400",
    name: "Asset disposal losses",
    category: "other_expense",
    controlKind: "none",
  },
];
