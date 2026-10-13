"use client";
import { RecordLink } from "@/components/records/record-link";
import { InlineNotice } from "@/components/ui/foundation";

import { FormDisclosure } from "@/components/ui/form-disclosure";
import { Button, Field, Select, TextArea, TextInput, Table, TableFrame, SectionHeader } from "@/components/ui/foundation";

import { DraftFormDrawer } from "@/components/ui/draft-form-drawer";
import { StatusBadge } from "@/components/ui/primitives";
import { useRef, useState } from "react";
import { useRouter } from "next/navigation";
import { workFetch } from "@/lib/work_projection/work-fetch";
import { minorToDecimal } from "@/lib/finance/finance-money";
import type { pricingWorkspace } from "@/server/modules/sales/public/sales-pricing";
import { FormGrid, Stack, Card, FormLayout } from "@/components/ui/foundation";

type Data = Awaited<ReturnType<typeof pricingWorkspace>>;
type DraftItem = { productId: string; price: string; taxBps: number };
function DraftPriceListEditor({
  data,
  list,
  listItems,
  busy,
  onSave,
}: {
  data: Data;
  list: Data["lists"][number];
  listItems: Data["items"];
  busy: boolean;
  onSave: (input: unknown) => Promise<boolean>;
}) {
  const [open, setOpen] = useState(false),
    [dirty, setDirty] = useState(false),
    [items, setItems] = useState<DraftItem[]>(() =>
      listItems.map((item) => ({
        productId: String(item.product_id),
        price: minorToDecimal(Number(item.price_minor)),
        taxBps: Number(item.tax_bps),
      })),
    );
  function add() {
    const product = data.products.find((p) => !items.some((item) => item.productId === p.id));
    if (product)
      setItems([
        ...items,
        { productId: String(product.id), price: "0.00", taxBps: Number(product.tax_bps) },
      ]);
  }
  return (
    <>
      <Button appearance="standard" variant="secondary" type="button" onClick={() => setOpen(true)}>Edit draft</Button>
      <DraftFormDrawer appearance="standard" open={open} dirty={dirty} onClose={() => setOpen(false)} onDiscard={() => { setDirty(false); setOpen(false); setItems(listItems.map(item => ({ productId: String(item.product_id), price: minorToDecimal(Number(item.price_minor)), taxBps: Number(item.tax_bps) }))); }} title="Edit draft price list" dismissDisabled={busy}>
      <div onChangeCapture={() => setDirty(true)} onClickCapture={event => { if ((event.target as Element).closest("button")) setDirty(true); }}>
    <form
      className="ui-stack"
      onSubmit={async (event) => {
        event.preventDefault();
        const form = new FormData(event.currentTarget);
        if (
          await onSave({
            expectedVersion: Number(list.version),
            name: form.get("name"),
            currency: data.currency,
            customerId: form.get("customerId") || null,
            validFrom: form.get("validFrom"),
            validUntil: form.get("validUntil"),
            maxAutoDiscountBps: Math.round(Number(form.get("maxAutoDiscountPercent")) * 100),
            items,
          })
        )
          { setDirty(false); setOpen(false); }
      }}
    >
      <h4>Edit draft safely</h4>
      <p className="muted">
        Saving creates a new version. If someone changed this draft after you opened it, ERPX keeps
        their work and asks you to reload.
      </p>
      <FormGrid as="div" className="ui-form-grid">
        <Field appearance="standard">
          Name
          <TextInput appearance="standard"
            name="name"
            required
            minLength={3}
            maxLength={160}
            defaultValue={String(list.name)}
          />
        </Field>
        <Field appearance="standard">
          Customer scope
          <Select appearance="standard" name="customerId" defaultValue={String(list.customer_id ?? "")}>
            <option value="">All customers</option>
            {data.customers.map((customer) => (
              <option key={String(customer.id)} value={String(customer.id)}>
                {String(customer.code)} · {String(customer.name)}
              </option>
            ))}
          </Select>
        </Field>
        <Field appearance="standard">
          Valid from
          <TextInput appearance="standard" name="validFrom" type="date" required defaultValue={String(list.valid_from)} />
        </Field>
        <Field appearance="standard">
          Valid until
          <TextInput appearance="standard"
            name="validUntil"
            type="date"
            required
            defaultValue={String(list.valid_until)}
          />
        </Field>
        <Field appearance="standard">
          Automatic discount limit (%)
          <TextInput appearance="standard"
            name="maxAutoDiscountPercent"
            type="number"
            min="0"
            max="100"
            step="0.01"
            defaultValue={Number(list.max_auto_discount_bps) / 100}
            required
          />
        </Field>
      </FormGrid>
      {items.map((item, index) => (
        <FormGrid as="div" className="ui-form-grid" key={`${item.productId}-${index}`}>
          <Field appearance="standard">
            Product {index + 1}
            <Select appearance="standard"
              value={item.productId}
              onChange={(event) => {
                const product = data.products.find((p) => p.id === event.target.value)!;
                setItems(
                  items.map((row, i) =>
                    i === index
                      ? { ...row, productId: String(product.id), taxBps: Number(product.tax_bps) }
                      : row,
                  ),
                );
              }}
            >
              {data.products.map((product) => (
                <option key={String(product.id)} value={String(product.id)}>
                  {String(product.sku)} · {String(product.name)}
                </option>
              ))}
            </Select>
          </Field>
          <Field appearance="standard">
            Approved unit price ({data.currency})
            <TextInput appearance="standard"
              type="number"
              min="0"
              step="0.01"
              required
              value={item.price}
              onChange={(event) =>
                setItems(
                  items.map((row, i) =>
                    i === index ? { ...row, price: event.target.value } : row,
                  ),
                )
              }
            />
          </Field>
          <Field appearance="standard">
            Tax (%)
            <TextInput appearance="standard"
              type="number"
              min="0"
              max="100"
              step="0.01"
              required
              value={item.taxBps / 100}
              onChange={(event) =>
                setItems(
                  items.map((row, i) =>
                    i === index
                      ? { ...row, taxBps: Math.round(Number(event.target.value) * 100) }
                      : row,
                  ),
                )
              }
            />
          </Field>
          <Button appearance="standard"
            variant="secondary"
            type="button"
            onClick={() => setItems(items.filter((_, i) => i !== index))}
          >
            Remove
          </Button>
        </FormGrid>
      ))}
      <div className="ui-form-actions">
        <Button appearance="standard"
          variant="secondary"
          type="button"
          disabled={items.length >= data.products.length}
          onClick={add}
        >
          Add product
        </Button>
        <Button appearance="standard" variant="secondary" type="button" onClick={() => setOpen(false)}>
          Close editor
        </Button>
        <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy || !items.length}>
          {busy ? "Saving…" : "Save new draft version"}
        </Button>
      </div>
    </form>
      </div></DraftFormDrawer>
    </>
  );
}
export function SalesPricingConsole({ base, data }: { base: string; data: Data }) {
  const router = useRouter(),
    [busy, setBusy] = useState(false),
    [creating, setCreating] = useState(false),
    [dirty, setDirty] = useState(false),
    [error, setError] = useState(""),
    [items, setItems] = useState<DraftItem[]>([]),
    attempt = useRef<{ body: string; key: string } | null>(null);
  async function send(path: string, input: unknown) {
    const body = JSON.stringify(input);
    if (attempt.current?.body !== path + body)
      attempt.current = { body: path + body, key: crypto.randomUUID() };
    setBusy(true);
    setError("");
    try {
      const response = await workFetch(
          `${base.replace("/holdings/", "/api/v1/holdings/")}/commerce/pricing${path}`,
          {
            method: "POST",
            headers: { "Content-Type": "application/json", "Idempotency-Key": attempt.current.key },
            body,
          },
        ),
        result = await response.json();
      if (!response.ok) {
        if (response.status < 500) attempt.current = null;
        throw new Error(result.error?.message ?? "Unable to save pricing control.");
      }
      attempt.current = null;
      router.refresh();
      return true;
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : "Retry the pricing action.");
      return false;
    } finally {
      setBusy(false);
    }
  }
  function add() {
    const product = data.products.find((p) => !items.some((item) => item.productId === p.id));
    if (product)
      setItems([
        ...items,
        { productId: String(product.id), price: "0.00", taxBps: Number(product.tax_bps) },
      ]);
  }
  return (
    <Stack as="div" className="ui-stack">
      {error && (
        <InlineNotice tone="danger" className="error" role="alert">
          {error}
        </InlineNotice>
      )}
      <SectionHeader><div><h2 className="ui-section-title">Price lists</h2><p className="ui-description">Approved prices, effective dates and independent review.</p></div><Button appearance="standard" type="button" variant="primary" onClick={() => setCreating(true)}>Prepare price list</Button></SectionHeader>
      <DraftFormDrawer appearance="standard" open={creating} dirty={dirty} onClose={() => setCreating(false)} onDiscard={() => { setDirty(false); setItems([]); setCreating(false); }} title="Prepare price list" dismissDisabled={busy}>
      <div onChangeCapture={() => setDirty(true)} onClickCapture={event => { if ((event.target as Element).closest("button")) setDirty(true); }}>
        <p className="muted">
          Create a dated company or customer price list. Another Company sales manager must activate
          it before quotations or orders use it.
        </p>
        <form
          onSubmit={async (event) => {
            event.preventDefault();
            const form = new FormData(event.currentTarget);
            if (
              await send("", {
                name: form.get("name"),
                currency: data.currency,
                customerId: form.get("customerId") || null,
                validFrom: form.get("validFrom"),
                validUntil: form.get("validUntil"),
                maxAutoDiscountBps: Math.round(Number(form.get("maxAutoDiscountPercent")) * 100),
                items,
              })
            )
              { setItems([]); setCreating(false); }
          }}
        >
          <FormGrid as="div" className="ui-form-grid">
            <Field appearance="standard">
              Name
              <TextInput appearance="standard" name="name" required minLength={3} maxLength={160} />
            </Field>
            <Field appearance="standard">
              Customer scope
              <Select appearance="standard" name="customerId" defaultValue="">
                <option value="">All customers</option>
                {data.customers.map((customer) => (
                  <option key={String(customer.id)} value={String(customer.id)}>
                    {String(customer.code)} · {String(customer.name)}
                  </option>
                ))}
              </Select>
            </Field>
            <Field appearance="standard">
              Valid from
              <TextInput appearance="standard" name="validFrom" type="date" required />
            </Field>
            <Field appearance="standard">
              Valid until
              <TextInput appearance="standard" name="validUntil" type="date" required />
            </Field>
            <Field appearance="standard">
              Automatic discount limit (%)
              <TextInput appearance="standard"
                name="maxAutoDiscountPercent"
                type="number"
                min="0"
                max="100"
                step="0.01"
                defaultValue="0"
                required
              />
            </Field>
          </FormGrid>
          {items.map((item, index) => (
            <FormGrid as="div" className="ui-form-grid" key={index}>
              <Field appearance="standard">
                Product {index + 1}
                <Select appearance="standard"
                  value={item.productId}
                  onChange={(event) => {
                    const product = data.products.find((p) => p.id === event.target.value)!;
                    setItems(
                      items.map((row, i) =>
                        i === index
                          ? {
                              ...row,
                              productId: String(product.id),
                              taxBps: Number(product.tax_bps),
                            }
                          : row,
                      ),
                    );
                  }}
                >
                  {data.products.map((product) => (
                    <option key={String(product.id)} value={String(product.id)}>
                      {String(product.sku)} · {String(product.name)}
                    </option>
                  ))}
                </Select>
              </Field>
              <Field appearance="standard">
                Approved unit price ({data.currency})
                <TextInput appearance="standard"
                  type="number"
                  min="0"
                  step="0.01"
                  required
                  value={item.price}
                  onChange={(event) =>
                    setItems(
                      items.map((row, i) =>
                        i === index ? { ...row, price: event.target.value } : row,
                      ),
                    )
                  }
                />
              </Field>
              <Field appearance="standard">
                Tax (%)
                <TextInput appearance="standard"
                  type="number"
                  min="0"
                  max="100"
                  step="0.01"
                  required
                  value={item.taxBps / 100}
                  onChange={(event) =>
                    setItems(
                      items.map((row, i) =>
                        i === index
                          ? { ...row, taxBps: Math.round(Number(event.target.value) * 100) }
                          : row,
                      ),
                    )
                  }
                />
              </Field>
              <Button appearance="standard"
                variant="secondary"
                type="button"
                onClick={() => setItems(items.filter((_, i) => i !== index))}
              >
                Remove
              </Button>
            </FormGrid>
          ))}
          <div className="ui-form-actions">
            <Button appearance="standard"
              variant="secondary"
              type="button"
              disabled={items.length >= data.products.length}
              onClick={add}
            >
              Add product
            </Button>
            <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy || !items.length}>
              {busy ? "Saving…" : "Save draft price list"}
            </Button>
          </div>
        </form>
      </div></DraftFormDrawer>
      <Card as="section" appearance="standard" className="ui-panel">
        <SectionHeader><h2>Price-list history</h2><span className="ui-badge">{data.lists.length}</span></SectionHeader>
        <TableFrame appearance="standard" label="Price-list history">
          <Table>
            <thead><tr><th>Price list</th><th>Products / unit prices</th><th>Customer scope</th><th>Effective dates</th><th data-align="right">Automatic discount</th><th>Version</th><th>Status</th><th>Actions</th></tr></thead>
            <tbody>
              {data.lists.map((list) => {
                const listItems = data.items.filter((item) => item.price_list_id === list.id);
                return <tr key={String(list.id)}>
                  <td><RecordLink href={`${base}/record-details/price_list/${encodeURIComponent(String(list.id))}`}>{String(list.name)}</RecordLink></td>
                  <td><div className="grid gap-3">{listItems.map(item => <div key={String(item.id)}>
                    <div>{String(item.sku)}</div>
                    <div className="ui-description">{String(item.product_name)}</div>
                    <div className="ui-description">{String(list.currency)} {minorToDecimal(Number(item.price_minor))} · tax {Number(item.tax_bps) / 100}%</div>
                  </div>)}{!listItems.length && "—"}</div></td>
                  <td>{String(list.customer_name ?? "All customers")}</td>
                  <td className="whitespace-nowrap">{String(list.valid_from)}<div className="ui-description">to {String(list.valid_until)}</div></td>
                  <td data-align="right">{Number(list.max_auto_discount_bps) / 100}%</td>
                  <td>v{String(list.version)}</td>
                  <td><StatusBadge appearance="standard" status={String(list.status)} /></td>
                  <td>
                    <div className="flex flex-wrap gap-2">
                      {list.status === "draft" && <DraftPriceListEditor data={data} list={list} listItems={listItems} busy={busy} onSave={(input) => send(`/${list.id}/edit`, input)} />}
                      {list.status === "draft" && data.canApprove && <FormDisclosure appearance="standard" drawerTitle={`Review ${String(list.name)}`} dismissDisabled={busy}>
                        <summary className="ui-standard-control cursor-pointer rounded-[var(--ui-radius-md)] border border-[var(--ui-line)] px-4 py-2">Review draft</summary>
                        <FormLayout className="ui-form-grid" onSubmit={(event) => {
                          event.preventDefault();
                          const form = new FormData(event.currentTarget);
                          void send(`/${list.id}`, { expectedVersion: Number(list.version), action: form.get("action"), reason: form.get("reason") });
                        }}>
                          <Field appearance="standard">Decision<Select appearance="standard" name="action"><option value="activate">Activate</option><option value="cancel">Cancel draft</option></Select></Field>
                          <Field appearance="standard">Independent review reason<TextArea appearance="standard" name="reason" required minLength={5} maxLength={1000} /></Field>
                          <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy}>Record decision</Button>
                        </FormLayout>
                      </FormDisclosure>}
                    </div>
                  </td>
                </tr>;
              })}
              {!data.lists.length && <tr><td colSpan={8}>No price lists have been prepared.</td></tr>}
            </tbody>
          </Table>
        </TableFrame>

      </Card>
    </Stack>
  );
}
