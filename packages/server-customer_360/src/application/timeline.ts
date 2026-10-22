import { publicTimelineItems, type CustomerActivityItem, type TimelineCursor } from "@erpx/server-customer_360/domain/timeline";

export type CustomerTimelineRepository = {
  visibleCustomer(id: string): Promise<{ id: string; name: string; code: string } | null>;
  events(id: string, cursor: TimelineCursor | null, limit: number): Promise<CustomerActivityItem[]>;
};
export function createCustomerTimeline(ports: {
  requireRead(): void; parseQuery(raw: unknown): { cursor?: string; limit: number };
  decodeCursor(value: string): TimelineCursor; encodeCursor(value: TimelineCursor): string;
  repository(): Promise<CustomerTimelineRepository>; coverage(): string[];
  error(status: number, code: string, message: string): Error;
}) {
  return async (customerId: string, raw: unknown = {}) => {
    ports.requireRead(); const query = ports.parseQuery(raw);
    let cursor: TimelineCursor | null = null;
    if (query.cursor) {
      try { cursor = ports.decodeCursor(query.cursor); }
      catch { throw ports.error(422, "TIMELINE_CURSOR_INVALID", "Use a cursor returned by this customer timeline."); }
    }
    const repository = await ports.repository(), customer = await repository.visibleCustomer(customerId);
    if (!customer) throw ports.error(404, "CUSTOMER_TIMELINE_NOT_FOUND", "Customer not found in this scope.");
    const rows = await repository.events(customerId, cursor, query.limit + 1), items = rows.slice(0, query.limit), last = items.at(-1), hasMore = rows.length > query.limit;
    return { customer, items: publicTimelineItems(items), hasMore, nextCursor: hasMore && last ? ports.encodeCursor({ at: Number(last.at), id: last.id }) : null,
      coverage: ports.coverage(), limitations: "Internal staff projection. Source records remain authoritative. Order/return entries show their latest state, not a complete transition log. Field Service, Finance and Legal entries show scoped source state; Portal and customer-authority records are not included." };
  };
}
