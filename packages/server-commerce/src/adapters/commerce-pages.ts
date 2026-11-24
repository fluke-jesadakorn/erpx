
import { requireCommercePurchasedRead } from "@erpx/server-commerce/adapters/purchase-boundary";

import { commerceFinancePageSources } from "@erpx/server-finance/public/commerce-page-sources";
import { commerceInventoryPageSources } from "@erpx/server-inventory/public/commerce-page-sources";

import { readListPage, readListPageRows } from "@erpx/server-work_projection/public/list-page";
import { nativeCommerceReadAll, RD_PRODUCT_PAGE_ROWS } from './native-commerce-read';
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { requirePermission } from "@erpx/server-identity_access/public/iam-permissions";

import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import { holdingResources } from "../../../../packages/server-runtime/src/public/runtime";

import { type Row } from "@erpx/server-commerce/application/commerce";

type CommercePageRow = Row & { id: string; page_key: string };
const pageOrder = (a: CommercePageRow, b: CommercePageRow) => {
    const ak = String(a.page_key), bk = String(b.page_key);
    return ak < bk ? -1 : ak > bk ? 1 : a.id < b.id ? -1 : a.id > b.id ? 1 : 0;
};
/** r19: the product page is planned by the mounted owner command-plan port and
 * executed on the caller's unit. The retained TypeScript cursor/limit policy is
 * applied in the same order the retired `WITH list AS (...) WHERE (page_key,id)
 * </> (?,?) ORDER BY page_key,id LIMIT ?` wrapper enforced. */
function nativeProductPage(write: SqlDatabase, actor: WorkContext, policy: {
    scope: unknown;
    cursor?: string;
    descending?: boolean;
    limit?: number;
}) {
    return readListPageRows<CommercePageRow>(policy, async ({ cursor, descending, limit }) => {
        const rows = await nativeCommerceReadAll<CommercePageRow>(RD_PRODUCT_PAGE_ROWS, [actor.companyId], write);
        rows.sort(pageOrder);
        if (descending) rows.reverse();
        const paged = cursor ? rows.filter(row => {
            const cmp = pageOrder(row, { id: cursor.id, page_key: cursor.key } as CommercePageRow);
            return descending ? cmp < 0 : cmp > 0;
        }) : rows;
        return paged.slice(0, limit);
    });
}

export async function commercePages(write: SqlDatabase, actor: WorkContext, query: {
    view: string;
    kind?: string;
    q?: string;
    cursor?: string;
    movementCursor?: string;
    new?: string;
    edit?: string;
    request?: string;
    status?: string;
    startDate?: string;
    endDate?: string;
}) {
    const s = { sql: 'o.company_id=? AND (?::text IS NULL OR o.branch_id=?) AND (?::text IS NULL OR o.department_id=?)', args: [actor.companyId, actor.branchId, actor.branchId, actor.departmentId, actor.departmentId] };
    const search = query.q ?? '', branchArgs = [actor.companyId, actor.branchId, actor.branchId];
    const partyKind = query.kind === 'customer' || query.kind === 'supplier' ? query.kind : '';
    const orderView = ['sales', 'quote', 'purchase'].includes(query.view);
    const lists: Record<string, {
        sql: string;
        args: unknown[];
        descending?: boolean;
    }> = {
        orders: { sql: `SELECT o.*,p.name party_name,o.created_at::text page_key FROM commerce_orders o ${commerceFinancePageSources.orderPartyJoin}
      WHERE ${s.sql} AND o.kind=? AND (?='' OR strpos(lower(o.number||' '||p.name||' '||o.state),lower(?))>0) AND (?='' OR o.state=?) AND (?='' OR o.order_date>=?) AND (?='' OR o.order_date<=?)`, args: [...s.args, query.view, search, search, query.status ?? '', query.status ?? '', query.startDate ?? '', query.startDate ?? '', query.endDate ?? '', query.endDate ?? ''], descending: true },
        products: { sql: 'SELECT	*,sku AS page_key FROM commerce_products WHERE company_id=?', args: [actor.companyId] },
        parties: commerceFinancePageSources.parties(actor.companyId, partyKind),
        warehouses: { sql: 'SELECT *,name AS page_key FROM commerce_warehouses WHERE company_id=? AND (?::text IS NULL OR branch_id=?)', args: branchArgs },
        stock: commerceInventoryPageSources.stock(branchArgs),
    };
    const list = orderView ? 'orders' : query.view;
    const pageScope = [actor.companyId, actor.branchId, list, query.view, ...(list === 'parties' ? [partyKind] : []), search, ...(query.status ? [query.status] : []), query.startDate ?? '', query.endDate ?? ''];
    const [page, movementPage] = await Promise.all([
        list === 'products'
            ? nativeProductPage(write, actor, { cursor: query.cursor, scope: pageScope })
            : readListPage<Row & {
                id: string;
                page_key: string;
            }>(write, { ...lists[list], cursor: query.cursor, scope: pageScope }),
        query.view === 'stock' ? readListPage<Row & {
            id: string;
            page_key: string;
        }>(write, commerceInventoryPageSources.movements(branchArgs, query.movementCursor, [actor.companyId, actor.branchId, 'movements'])) : null,
    ]);
    return { page, movementPage };
}
export async function commerceSalesDocuments(actor: WorkContext, query: { view: "sales" | "quote"; q?: string; status?: string; startDate?: string; endDate?: string; cursor?: string }) {
    requirePermission(actor, "commerce.read");
    const database=(await holdingResources(actor.holdingId)).write;
    await requireCommercePurchasedRead(database,actor,query.view);
    return (await commercePages(database, actor, query)).page;
}
