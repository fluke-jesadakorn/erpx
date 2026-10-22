import type {Row} from '@erpx/server-commerce/public/commerce';
export type CustomerSummary = {
  nextFollowup: { id: string; dueDate: string; action: string; orderId: string } | null;
  openOrders: number;
  openOpportunities: number | null;
  openSupport: number | null;
};
export type CustomerDirectoryRow = Row & { id: string; page_key: string; summary: CustomerSummary };

