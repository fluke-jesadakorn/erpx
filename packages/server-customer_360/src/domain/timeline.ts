export type CustomerTimelineItem = { id: string; source_id: string; source_type: string; title: string; status: string; version: number; at: number; href: string };
export type CustomerActivityItem = CustomerTimelineItem & { customer_id: string; customer_name: string };
export type TimelineCursor = { at: number; id: string };
export function publicTimelineItems(rows: CustomerActivityItem[]) {
  return rows.map(item => ({ id: item.id, source_id: item.source_id, source_type: item.source_type, title: item.title, status: item.status, version: item.version, at: item.at, href: item.href }));
}
