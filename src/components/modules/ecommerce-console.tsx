"use client";
import { useRef, useState } from "react";
import { useRouter } from "next/navigation";
import { workFetch } from "@/lib/work-fetch";
import type { ecommerceWorkspace } from "@/server/ecommerce";

type Workspace = Awaited<ReturnType<typeof ecommerceWorkspace>>;
export function EcommerceConsole({
  base,
  data,
}: {
  base: string;
  data: Workspace;
}) {
  const router = useRouter();
  const last = useRef<{ body: string; key: string } | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  async function send(input: unknown) {
    const body = JSON.stringify(input);
    if (last.current?.body !== body)
      last.current = { body, key: crypto.randomUUID() };
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
        throw new Error(
          result.error?.message ?? "Unable to update E-commerce.",
        );
      }
      last.current = null;
      if (
        input &&
        typeof input === "object" &&
        "entity" in input &&
        input.entity === "checkout"
      )
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
    <div className="content-stack">
      {error && <p role="alert">{error}</p>}
      <form
        className="card business-form"
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
        <label>
          Name
          <input name="name" required />
        </label>
        <label>
          Type
          <select name="channelType">
            <option value="web">Web</option>
            <option value="marketplace">Marketplace</option>
            <option value="social">Social</option>
          </select>
        </label>
        <label>
          Storefront reference
          <input name="storefront" required />
        </label>
        <button className="button" disabled={busy}>
          Create channel
        </button>
      </form>
      {data.channels
        .filter((x) => x.status === "draft")
        .map((x) => (
          <form
            className="card business-form"
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
            <label>
              Review reason
              <textarea name="reason" required minLength={3} />
            </label>
            <button className="button" disabled={busy}>
              Activate
            </button>
          </form>
        ))}
      <form
        className="card business-form"
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
        <label>
          Channel
          <select name="channelId">
            {channels.map((x) => (
              <option key={String(x.id)} value={String(x.id)}>
                {String(x.name)}
              </option>
            ))}
          </select>
        </label>
        <label>
          Product
          <select name="productId">
            {data.products.map((x) => (
              <option key={String(x.id)} value={String(x.id)}>
                {String(x.name)}
              </option>
            ))}
          </select>
        </label>
        <label>
          Title
          <input name="title" required />
        </label>
        <label>
          Currency
          <input name="currency" defaultValue="THB" required />
        </label>
        <label>
          Price
          <input name="price" inputMode="decimal" required />
        </label>
        <button className="button" disabled={busy}>
          Create listing
        </button>
      </form>
      {data.listings
        .filter((x) => x.status === "draft")
        .map((x) => (
          <form
            className="card business-form"
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
            <label>
              Review reason
              <textarea name="reason" required minLength={3} />
            </label>
            <button className="button" disabled={busy}>
              Publish
            </button>
          </form>
        ))}
      <form
        className="card business-form"
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
        <label>
          Channel
          <select name="channelId">
            {channels.map((x) => (
              <option key={String(x.id)} value={String(x.id)}>
                {String(x.name)}
              </option>
            ))}
          </select>
        </label>
        <label>
          Customer
          <select name="partyId">
            {data.parties.map((x) => (
              <option key={String(x.id)} value={String(x.id)}>
                {String(x.name)}
              </option>
            ))}
          </select>
        </label>
        <label>
          Warehouse
          <select name="warehouseId">
            {data.warehouses.map((x) => (
              <option key={String(x.id)} value={String(x.id)}>
                {String(x.name)}
              </option>
            ))}
          </select>
        </label>
        <label>
          Currency
          <input name="currency" defaultValue="THB" required />
        </label>
        <button className="button" disabled={busy}>
          Open cart
        </button>
      </form>
      {data.carts
        .filter((x) => x.status === "open")
        .map((cart) => (
          <section className="card" key={String(cart.id)}>
            <h2>Cart · {String(cart.id)}</h2>
            <form
              className="business-form"
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
              <label>
                Listing
                <select name="listingId">
                  {listings.map((x) => (
                    <option key={String(x.id)} value={String(x.id)}>
                      {String(x.title)}
                    </option>
                  ))}
                </select>
              </label>
              <label>
                Quantity
                <input name="quantity" defaultValue="1" required />
              </label>
              <button className="button" disabled={busy}>
                Add line
              </button>
            </form>
            <form
              className="business-form"
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
              <label>
                Order date
                <input name="orderDate" type="date" required />
              </label>
              <label>
                Due date
                <input name="dueDate" type="date" required />
              </label>
              <button className="button" disabled={busy}>
                Checkout to Sales
              </button>
            </form>
          </section>
        ))}
    </div>
  );
}
