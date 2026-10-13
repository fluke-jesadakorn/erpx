import { ActionLink as UiActionLink } from "@/components/ui/foundation";
import { Button, Field, Select, TextInput } from "@/components/ui/foundation";
import {
  FeaturePage,
  FeatureTabs,
  FeatureWidget,
  WidgetGrid,
} from "@/components/shared/features/feature-page";
import { ListPagination } from "@/components/ui/list-pagination";
import { PageHeader, StatusBadge } from "@/components/ui/primitives";
import { TransferEditor } from "@/components/workplace/work-editors";
import { minorToDecimal } from "@/lib/finance/finance-money";
import { workspaceBase } from "@/lib/work_projection/workspace";
import { publicScope, requireWorkContext } from "@/server/modules/identity_access/public/authorization";
import { canPrepareTransfers, transferOptions, transfersPage } from "@/server/modules/inventory/public/inventory-transfers";
import { RecordLink as Link } from "@/components/records/record-link";
import { pageError } from "@/server/modules/work_projection/public/page-error";
import { FilterBar, Table, TableFrame } from "@/components/ui/foundation";
export const dynamic = "force-dynamic";
export default async function Transfers({
  params,
  searchParams,
}: {
  params: Promise<{ holdingSlug: string }>;
  searchParams: Promise<{ q?: string; status?: string; cursor?: string; new?: string }>;
}) {
  const { holdingSlug } = await params,
    actor = await requireWorkContext(holdingSlug),
    base = workspaceBase(publicScope(actor));
  const search = await searchParams;
  const [page, options] = await Promise.all([
    transfersPage(actor, search).catch((error) => pageError(error, holdingSlug)),
    search.new === "1" && canPrepareTransfers(actor)
      ? transferOptions(actor).catch((error) => pageError(error, holdingSlug))
      : null,
  ]);
  const filtered = page.items;
  return (
    <FeaturePage className="ui-page-content" data-source="live">
      <PageHeader appearance="standard"
        eyebrow="Inventory · Operations"
        title="Warehouse transfers"
        description="Move received stock between company warehouses with independent review and preserved valuation."
        action={
          <UiActionLink appearance="standard" variant="secondary" href={`${base}/commerce?view=stock`}>
            Stock and movements
          </UiActionLink>
        }
      />
      <FeatureTabs appearance="standard" />
      <WidgetGrid pageKey="operations-transfers">
        {canPrepareTransfers(actor) &&
          (options ? (
            <FeatureWidget appearance="standard" id="operations-transfers-warehouse-transfer" title="Warehouse transfer">
              <TransferEditor base={base} options={options} initialOpen />
            </FeatureWidget>
          ) : (
            <UiActionLink appearance="standard" variant="primary" href="?new=1">
              New transfer
            </UiActionLink>
          ))}
        <FilterBar className="ui-filter-bar">
          <Field appearance="standard">
            Search transfers
            <TextInput appearance="standard"
              name="q"
              defaultValue={search.q}
              placeholder="Transfer, warehouse or purpose"
            />
          </Field>
          <Field appearance="standard">
            Transfer status
            <Select appearance="standard" name="status" defaultValue={search.status ?? ""}>
              <option value="">All statuses</option>
              {["draft", "submitted", "rejected", "posted", "cancelled"].map((s) => (
                <option key={s} value={s}>
                  {s}
                </option>
              ))}
            </Select>
          </Field>
          <Button appearance="standard" type="submit" variant="secondary">
            Filter transfers
          </Button>
        </FilterBar>
        <FeatureWidget appearance="standard" id="operations-transfers-transfer-register" title="Transfer register">
          <p className="ui-description">{filtered.length} transfers on this page</p>
          <TableFrame appearance="standard" label="Transfer register">
            <Table>
              <thead><tr><th>Document</th><th>From warehouse</th><th>To warehouse</th><th>Date / purpose</th><th data-align="right">Value</th><th>Status</th></tr></thead>
              <tbody>
                {filtered.map((t) => <tr key={String(t.id)}>
                  <td><Link href={`${base}/stock-transfers/${t.id}`}>{String(t.number)}</Link></td>
                  <td>{String(t.source_warehouse_name)}</td>
                  <td>{String(t.target_warehouse_name)}</td>
                  <td>{String(t.transfer_date)}<div className="ui-description">{String(t.reason)}</div></td>
                  <td data-align="right" className="whitespace-nowrap">{String(t.currency)} {minorToDecimal(Number(t.total_value_minor))}</td>
                  <td><StatusBadge appearance="standard" status={t.delivery_mode === "transit" && t.status === "posted" ? "transport_approved" : String(t.status)} /></td>
                </tr>)}
                {!filtered.length && <tr><td colSpan={6}>No transfers match this view.</td></tr>}
              </tbody>
            </Table>
          </TableFrame>
        </FeatureWidget>
        <ListPagination {...page} query={search} count={filtered.length} />
      </WidgetGrid>
    </FeaturePage>
  );
}
