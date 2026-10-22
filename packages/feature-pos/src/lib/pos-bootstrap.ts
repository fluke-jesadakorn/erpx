import type { PosAccountKey } from "@erpx/server-pos/public/rules/pos";

export const posCommerceAccountDefaults = [
  { key: 'cash', field: 'cash_account_id', label: 'Cash', category: 'asset' },
  { key: 'receivable', field: 'receivable_account_id', label: 'POS receivables', category: 'asset' },
  { key: 'revenue', field: 'revenue_account_id', label: 'Revenue', category: 'revenue' },
  { key: 'outputTax', field: 'output_tax_account_id', label: 'Output tax', category: 'liability' },
  { key: 'inventory', field: 'inventory_account_id', label: 'Inventory', category: 'asset' },
  { key: 'cogs', field: 'cogs_account_id', label: 'Cost of sales', category: 'expense' },
] as const satisfies ReadonlyArray<{
  key: PosAccountKey;
  field: string;
  label: string;
  category: string;
}>;

export const posGeneratedAccountSpecs = [
  { key: 'bank', code: 'POS-BANK', name: 'POS bank deposits', category: 'asset', controlKind: 'bank' },
  { key: 'edc', code: 'POS-EDC-CLEARING', name: 'POS EDC clearing', category: 'asset', controlKind: 'none' },
  { key: 'stripe', code: 'POS-STRIPE-CLEARING', name: 'POS Stripe clearing', category: 'asset', controlKind: 'none' },
  { key: 'advance', code: 'POS-CUSTOMER-ADVANCES', name: 'POS customer advances', category: 'liability', controlKind: 'none' },
  { key: 'wip', code: 'POS-WORK-IN-PROGRESS', name: 'POS work in progress', category: 'asset', controlKind: 'none' },
  { key: 'waste', code: 'POS-WASTE-EXPENSE', name: 'POS waste expense', category: 'expense', controlKind: 'none' },
  { key: 'refund', code: 'POS-CUSTOMER-REFUNDS', name: 'POS customer refunds payable', category: 'liability', controlKind: 'none' },
  { key: 'fees', code: 'POS-FEES', name: 'POS fees and losses', category: 'expense', controlKind: 'none' },
  { key: 'cashDifference', code: 'POS-CASH-VARIANCE', name: 'POS cash variance', category: 'expense', controlKind: 'none' },
  { key: 'cashTransfer', code: 'POS-CASH-TRANSFERS', name: 'POS cash transfers', category: 'asset', controlKind: 'none' },
  { key: 'disputes', code: 'POS-DISPUTES', name: 'POS disputes pending', category: 'asset', controlKind: 'none' },
] as const satisfies ReadonlyArray<{
  key: PosAccountKey;
  code: string;
  name: string;
  category: string;
  controlKind: 'none' | 'bank';
}>;
