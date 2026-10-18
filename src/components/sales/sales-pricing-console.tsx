"use client";
import { canonicalUiHref } from "@/lib/work_projection/destination-registry";
import { RecordLink } from "@/components/records/record-link";

import { Button, Field, Select, TextArea, TextInput, Table, TableFrame, SectionHeader } from "@/components/ui/foundation";

import {CRMCommandFeedback,CRMCommandState,CRMDraftBoundary,CRMEditor,CRMForm,useCRMSnapshot} from "@/components/crm/draft-boundary";
import {usePricingCommand} from "./use-pricing-command";
import { StatusBadge } from "@/components/ui/primitives";
import { useState } from "react";
import { minorToDecimal } from "@/lib/finance/finance-money";
import type { pricingWorkspace } from "@/server/modules/sales/public/sales-pricing";
import { FormGrid, Stack, Card } from "@/components/ui/foundation";

type Data = Awaited<ReturnType<typeof pricingWorkspace>>;
type DraftItem = { productId: string; price: string; taxBps: number };
function DraftPriceListEditor({
  data,
  list,
  listItems,
  busy,
  onSave,
  allowed,
  currentVersion,
}: {
  data: Data;
  list: Data["lists"][number];
  listItems: Data["items"];
  busy: boolean;
  onSave: (input: unknown) => Promise<void>;
  allowed: boolean;
  currentVersion?: string | number;
}) {
  const [items, setItems] = useState<DraftItem[]>(() =>
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
      <CRMEditor title="Edit draft price list" label="Edit draft" disabled={!allowed} onDiscard={() => setItems(listItems.map(item => ({productId:String(item.product_id),price:minorToDecimal(Number(item.price_minor)),taxBps:Number(item.tax_bps)})))} onAccepted={() => setItems(listItems.map(item => ({productId:String(item.product_id),price:minorToDecimal(Number(item.price_minor)),taxBps:Number(item.tax_bps)})))}>
    <CRMForm inline allowed={allowed} sourceVersion={currentVersion}
      className="ui-stack"
      onSubmit={(event) => {
        event.preventDefault();
        const form = new FormData(event.currentTarget);
        void onSave({
            expectedVersion: Number(list.version),
            name: form.get("name"),
            currency: data.currency,
            customerId: form.get("customerId") || null,
            validFrom: form.get("validFrom"),
            validUntil: form.get("validUntil"),
            maxAutoDiscountBps: Math.round(Number(form.get("maxAutoDiscountPercent")) * 100),
            items,
          });
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
        <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy || !items.length}>
          {busy ? "Saving…" : "Save new draft version"}
        </Button>
      </div>
    </CRMForm>
      </CRMEditor>
    </>
  );
}
export function SalesPricingConsole(props: {base:string;data:Data}) {
  const captured=useCRMSnapshot(props),{base,data}=captured.snapshot,purchase=props.data.purchase,newWork=purchase?.newWork!==false,settlement=purchase?.settleExisting!==false;
  const [items,setItems]=useState<DraftItem[]>([]);
  const allowed=(path:string,input:Record<string,unknown>)=>path===""||path.endsWith("/edit")?newWork:!!props.data.canApprove&&(input.action==="activate"?newWork:input.action==="cancel"?settlement:false);
  const command=usePricingCommand(base,allowed),{send,busy}=command;
  function add() {
    const product = data.products.find((p) => !items.some((item) => item.productId === p.id));
    if (product)
      setItems([
        ...items,
        { productId: String(product.id), price: "0.00", taxBps: Number(product.tax_bps) },
      ]);
  }
  return (
    <CRMDraftBoundary><div {...captured.rootProps}><CRMCommandState {...command}><Stack as="div" className="ui-stack">
      <CRMCommandFeedback/>
      <SectionHeader><div><h2 className="ui-section-title">Price lists</h2><p className="ui-description">Approved prices, effective dates and independent review.</p></div></SectionHeader>
      <CRMEditor title="Prepare price list" primary disabled={!newWork} onDiscard={() => setItems([])} onAccepted={() => setItems([])}>
        <p className="muted">
          Create a dated company or customer price list. Another Company sales manager must activate
          it before quotations or orders use it.
        </p>
        <CRMForm inline allowed={newWork} sourceVersion={props.data.currency}
          onSubmit={(event) => {
            event.preventDefault();
            const form = new FormData(event.currentTarget);
            void send("", {
                name: form.get("name"),
                currency: data.currency,
                customerId: form.get("customerId") || null,
                validFrom: form.get("validFrom"),
                validUntil: form.get("validUntil"),
                maxAutoDiscountBps: Math.round(Number(form.get("maxAutoDiscountPercent")) * 100),
                items,
              });
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
        </CRMForm>
      </CRMEditor>
      <Card as="section" appearance="standard" className="ui-panel">
        <SectionHeader><h2>Price-list history</h2><span className="ui-badge">{data.lists.length}</span></SectionHeader>
        <TableFrame appearance="standard" label="Price-list history">
          <Table>
            <thead><tr><th>Price list</th><th>Products / unit prices</th><th>Customer scope</th><th>Effective dates</th><th data-align="right">Automatic discount</th><th>Version</th><th>Status</th><th>Actions</th></tr></thead>
            <tbody>
              {data.lists.map((list) => {
                const listItems = data.items.filter((item) => item.price_list_id === list.id);
                return <tr key={String(list.id)}>
                  <td><RecordLink href={canonicalUiHref(`${base}/record-details/price_list/${encodeURIComponent(String(list.id))}`)}>{String(list.name)}</RecordLink></td>
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
                      {list.status === "draft" && <DraftPriceListEditor key={`${list.id}:${list.version}`} data={data} list={list} listItems={listItems} busy={busy} allowed={newWork} currentVersion={props.data.lists.find(row=>row.id===list.id)?.version??"unavailable"} onSave={(input) => send(`/${list.id}/edit`, input)} />}
                      {list.status === "draft" && props.data.canApprove && <CRMEditor key={`review:${list.id}:${list.version}`} title={`Review ${String(list.name)}`} label="Review draft" disabled={!newWork&&!settlement}>
                        <CRMForm inline allowed={newWork||settlement} sourceVersion={props.data.lists.find(row=>row.id===list.id)?.version??"unavailable"} className="ui-form-grid" onSubmit={(event) => {
                          event.preventDefault();
                          const form = new FormData(event.currentTarget);
                          void send(`/${list.id}`, { expectedVersion: Number(list.version), action: form.get("action"), reason: form.get("reason") });
                        }}>
                          <Field appearance="standard">Decision<Select appearance="standard" name="action" defaultValue={newWork?"activate":"cancel"}>{newWork&&<option value="activate">Activate</option>}{settlement&&<option value="cancel">Cancel draft</option>}</Select></Field>
                          <Field appearance="standard">Independent review reason<TextArea appearance="standard" name="reason" required minLength={5} maxLength={1000} /></Field>
                          <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy}>Record decision</Button>
                        </CRMForm>
                      </CRMEditor>}
                    </div>
                  </td>
                </tr>;
              })}
              {!data.lists.length && <tr><td colSpan={8}>No price lists have been prepared.</td></tr>}
            </tbody>
          </Table>
        </TableFrame>

      </Card>
    </Stack></CRMCommandState></div></CRMDraftBoundary>
  );
}
