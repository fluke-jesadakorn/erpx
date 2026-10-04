"use client";
import { InlineNotice } from "@/components/ui/foundation";

import { FormDisclosure } from "@/components/ui/form-disclosure";
import { Button, Field, Select, TextArea, TextInput } from "@/components/ui/foundation";

import { useRef, useState } from "react";
import { useRouter } from "next/navigation";
import { workFetch } from "@/lib/work-fetch";
import type { ecommerceWorkspace } from "@/server/ecommerce";
import { Stack, FormLayout, Card } from "@/components/ui/foundation";

type Workspace = Awaited<ReturnType<typeof ecommerceWorkspace>>;
export function EcommerceConsole({ base, data }: { base: string; data: Workspace }) {
  const router = useRouter();
  const last = useRef<{ body: string; key: string } | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  async function send(input: unknown) {
    const body = JSON.stringify(input);
    if (last.current?.body !== body) last.current = { body, key: crypto.randomUUID() };
    setBusy(true);
    setError("");
    try {
      const response = await workFetch(`/api/v1${base}/ecommerce`, {
        method: "POST",
        headers: {
          "Content-Type": "application/json",
          "Idempotency-Key": last.current.key,
        },
        body,
      });
      const result = await response.json();
      if (!response.ok) {
        if (response.status < 500) last.current = null;
        throw new Error(result.error?.message ?? "Unable to update E-commerce.");
      }
      last.current = null;
      if (input && typeof input === "object" && "entity" in input && input.entity === "checkout")
        router.push(`${base}/ecommerce/${result.data.id}`);
      router.refresh();
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : "Retry this action.");
    } finally {
      setBusy(false);
    }
  }
  const values = (event: React.FormEvent<HTMLFormElement>) =>
    Object.fromEntries(new FormData(event.currentTarget));
  const channels = data.channels.filter((x) => x.status === "active");
  const listings = data.listings.filter((x) => x.status === "published");
  return (
    <Stack as="div" className="ui-stack">
      {error && <InlineNotice tone="danger" role="alert">{error}</InlineNotice>}
      <FormLayout
        className="ui-panel ui-form-grid"
        onSubmit={(event) => {
          event.preventDefault();
          const v = values(event);
          void send({
            entity: "channel",
            name: v.name,
            channelType: v.channelType,
            configuration: { storefront: String(v.storefront) },
          });
        }}
      >
        <h2>Create channel draft</h2>
        <Field appearance="standard">
          Name
          <TextInput appearance="standard" name="name" required />
        </Field>
        <Field appearance="standard">
          Type
          <Select appearance="standard" name="channelType">
            <option value="web">Web</option>
            <option value="marketplace">Marketplace</option>
            <option value="social">Social</option>
          </Select>
        </Field>
        <Field appearance="standard">
          Storefront reference
          <TextInput appearance="standard" name="storefront" required />
        </Field>
        <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy}>
          Create channel
        </Button>
      </FormLayout>
      {data.channels
        .filter((x) => x.status === "draft")
        .map((x) => (
          <FormLayout
            className="ui-panel ui-form-grid"
            key={String(x.id)}
            onSubmit={(event) => {
              event.preventDefault();
              void send({
                entity: "channel",
                id: x.id,
                action: "activate",
                expectedVersion: Number(x.version),
                reason: String(new FormData(event.currentTarget).get("reason")),
              });
            }}
          >
            <h2>Review channel · {String(x.name)}</h2>
            <Field appearance="standard">
              Review reason
              <TextArea appearance="standard" name="reason" required minLength={3} />
            </Field>
            <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy}>
              Activate
            </Button>
          </FormLayout>
        ))}
      <FormDisclosure appearance="standard" className="ui-form-disclosure" dismissDisabled={busy}><summary>Create product listing</summary><FormLayout
        className="ui-panel ui-form-grid"
        onSubmit={(event) => {
          event.preventDefault();
          const v = values(event);
          void send({
            entity: "listing",
            channelId: v.channelId,
            productId: v.productId,
            title: v.title,
            currency: v.currency,
            price: v.price,
          });
        }}
      >
        <h2>Create product listing</h2>
        <Field appearance="standard">
          Channel
          <Select appearance="standard" name="channelId">
            {channels.map((x) => (
              <option key={String(x.id)} value={String(x.id)}>
                {String(x.name)}
              </option>
            ))}
          </Select>
        </Field>
        <Field appearance="standard">
          Product
          <Select appearance="standard" name="productId">
            {data.products.map((x) => (
              <option key={String(x.id)} value={String(x.id)}>
                {String(x.name)}
              </option>
            ))}
          </Select>
        </Field>
        <Field appearance="standard">
          Title
          <TextInput appearance="standard" name="title" required />
        </Field>
        <Field appearance="standard">
          Currency
          <TextInput appearance="standard" name="currency" defaultValue="THB" required />
        </Field>
        <Field appearance="standard">
          Price
          <TextInput appearance="standard" name="price" inputMode="decimal" required />
        </Field>
        <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy}>
          Create listing
        </Button>
      </FormLayout></FormDisclosure>
      {data.listings
        .filter((x) => x.status === "draft")
        .map((x) => (
          <FormLayout
            className="ui-panel ui-form-grid"
            key={String(x.id)}
            onSubmit={(event) => {
              event.preventDefault();
              void send({
                entity: "listing",
                id: x.id,
                action: "publish",
                expectedVersion: Number(x.version),
                reason: String(new FormData(event.currentTarget).get("reason")),
              });
            }}
          >
            <h2>Review listing · {String(x.title)}</h2>
            <Field appearance="standard">
              Review reason
              <TextArea appearance="standard" name="reason" required minLength={3} />
            </Field>
            <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy}>
              Publish
            </Button>
          </FormLayout>
        ))}
      <FormLayout
        className="ui-panel ui-form-grid"
        onSubmit={(event) => {
          event.preventDefault();
          const v = values(event);
          void send({
            entity: "cart",
            channelId: v.channelId,
            partyId: v.partyId,
            warehouseId: v.warehouseId,
            currency: v.currency,
          });
        }}
      >
        <h2>Open customer cart</h2>
        <Field appearance="standard">
          Channel
          <Select appearance="standard" name="channelId">
            {channels.map((x) => (
              <option key={String(x.id)} value={String(x.id)}>
                {String(x.name)}
              </option>
            ))}
          </Select>
        </Field>
        <Field appearance="standard">
          Customer
          <Select appearance="standard" name="partyId">
            {data.parties.map((x) => (
              <option key={String(x.id)} value={String(x.id)}>
                {String(x.name)}
              </option>
            ))}
          </Select>
        </Field>
        <Field appearance="standard">
          Warehouse
          <Select appearance="standard" name="warehouseId">
            {data.warehouses.map((x) => (
              <option key={String(x.id)} value={String(x.id)}>
                {String(x.name)}
              </option>
            ))}
          </Select>
        </Field>
        <Field appearance="standard">
          Currency
          <TextInput appearance="standard" name="currency" defaultValue="THB" required />
        </Field>
        <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy}>
          Open cart
        </Button>
      </FormLayout>
      {data.carts
        .filter((x) => x.status === "open")
        .map((cart) => (
          <Card as="section" appearance="standard" className="ui-panel" key={String(cart.id)}>
            <h2>Cart · {String(cart.id)}</h2>
            <FormLayout
              className="ui-form-grid"
              onSubmit={(event) => {
                event.preventDefault();
                const v = values(event);
                const listing = listings.find((x) => x.id === v.listingId);
                void send({
                  entity: "cart-line",
                  cartId: cart.id,
                  expectedVersion: Number(cart.version),
                  listingId: v.listingId,
                  listingVersion: Number(listing?.version),
                  quantity: v.quantity,
                });
              }}
            >
              <Field appearance="standard">
                Listing
                <Select appearance="standard" name="listingId">
                  {listings.map((x) => (
                    <option key={String(x.id)} value={String(x.id)}>
                      {String(x.title)}
                    </option>
                  ))}
                </Select>
              </Field>
              <Field appearance="standard">
                Quantity
                <TextInput appearance="standard" name="quantity" defaultValue="1" required />
              </Field>
              <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy}>
                Add line
              </Button>
            </FormLayout>
            <FormLayout
              className="ui-form-grid"
              onSubmit={(event) => {
                event.preventDefault();
                const v = values(event);
                void send({
                  entity: "checkout",
                  cartId: cart.id,
                  expectedVersion: Number(cart.version),
                  orderDate: v.orderDate,
                  dueDate: v.dueDate,
                });
              }}
            >
              <Field appearance="standard">
                Order date
                <TextInput appearance="standard" name="orderDate" type="date" required />
              </Field>
              <Field appearance="standard">
                Due date
                <TextInput appearance="standard" name="dueDate" type="date" required />
              </Field>
              <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy}>
                Checkout to Sales
              </Button>
            </FormLayout>
          </Card>
        ))}
    </Stack>
  );
}
