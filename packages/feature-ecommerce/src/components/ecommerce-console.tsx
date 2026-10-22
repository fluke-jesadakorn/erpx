"use client";
import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";
import {CRMForm as FormLayout,CRMDisclosure as FormDisclosure,CRMCommandState,CRMCommandFeedback,CRMDraftBoundary,useCRMSnapshot} from '@erpx/feature-crm/components/draft-boundary';
import {useCRMCommand} from '@erpx/feature-crm/components/use-crm-command';
import {ecommerceCommandAllowed} from '@erpx/feature-ecommerce/lib/purchase-controls';
import { Button, Field, Select, TextArea, TextInput } from "@erpx/ui/components/foundation";

import { useRouter } from "@erpx/feature-shared/lib/destination-router";
import type { ecommerceWorkspace } from "@erpx/server-ecommerce/public/ecommerce";
import { Stack, Card } from "@erpx/ui/components/foundation";

type Workspace = Awaited<ReturnType<typeof ecommerceWorkspace>>;
export function EcommerceConsole({ recordOnly=false, base, data:currentData }: { recordOnly?:boolean; base: string; data: Workspace }) {
  const router = useRouter();
  const {snapshot:data,rootProps}=useCRMSnapshot(currentData),purchase=currentData.purchase,active=purchase?.newWork!==false,review=purchase?.settleExisting!==false&&purchase?.canReview!==false,sourceVersion=JSON.stringify([currentData.channels.map(row=>[row.id,row.version]),currentData.listings.map(row=>[row.id,row.version]),currentData.carts.map(row=>[row.id,row.version])]);
  const command=useCRMCommand(base,'ecommerce',input=>ecommerceCommandAllowed(purchase,input),(receipt,input)=>{if(input.entity==='checkout')router.push(canonicalUiHref(`${base}/ecommerce/${receipt.id}`));}),{send,busy}=command;
  const values = (event: React.FormEvent<HTMLFormElement>) =>
    Object.fromEntries(new FormData(event.currentTarget));
  const channels = data.channels.filter((x) => x.status === "active");
  const listings = data.listings.filter((x) => x.status === "published");
  return (
    <CRMCommandState {...command}><CRMDraftBoundary><div {...rootProps}><Stack as="div" className="ui-stack">
      <CRMCommandFeedback/>
      {!recordOnly && <FormLayout sourceVersion={sourceVersion} allowed={active}
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
      </FormLayout>}
      {data.channels
        .filter((x) => ['draft','active'].includes(String(x.status)))
        .map((x) => (
          <FormLayout sourceVersion={sourceVersion} allowed={review}
            className="ui-panel ui-form-grid"
            key={String(x.id)}
            onSubmit={(event) => {
              event.preventDefault();
              void send({
                entity: "channel",
                id: x.id,
                action: String(new FormData(event.currentTarget).get('action')),
                expectedVersion: Number(x.version),
                reason: String(new FormData(event.currentTarget).get("reason")),
              });
            }}
          >
            <h2>Review channel · {String(x.name)}</h2>
            <Field appearance="standard">Decision<Select appearance="standard" name="action" defaultValue={x.status==='active'?'retire':active?'activate':'reject'}>{x.status==='active'?<option value="retire">Retire channel</option>:<><option value="activate" disabled={!active}>Activate channel</option><option value="reject">Reject channel</option></>}</Select></Field>
            <Field appearance="standard">
              Review reason
              <TextArea appearance="standard" name="reason" required minLength={3} />
            </Field>
            <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy}>
              Record channel decision
            </Button>
          </FormLayout>
        ))}
      {!recordOnly && <FormDisclosure appearance="standard" className="ui-form-disclosure" dismissDisabled={busy} disabled={!active}><summary>Create product listing</summary><FormLayout sourceVersion={sourceVersion} allowed={active}
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
      </FormLayout></FormDisclosure>}
      {data.listings
        .filter((x) => ['draft','published'].includes(String(x.status)))
        .map((x) => (
          <FormLayout sourceVersion={sourceVersion} allowed={review}
            className="ui-panel ui-form-grid"
            key={String(x.id)}
            onSubmit={(event) => {
              event.preventDefault();
              void send({
                entity: "listing",
                id: x.id,
                action: String(new FormData(event.currentTarget).get('action')),
                expectedVersion: Number(x.version),
                reason: String(new FormData(event.currentTarget).get("reason")),
              });
            }}
          >
            <h2>Review listing · {String(x.title)}</h2>
            <Field appearance="standard">Decision<Select appearance="standard" name="action" defaultValue={x.status==='published'?'retire':active?'publish':'reject'}>{x.status==='published'?<option value="retire">Retire listing</option>:<><option value="publish" disabled={!active}>Publish listing</option><option value="reject">Reject listing</option></>}</Select></Field>
            <Field appearance="standard">
              Review reason
              <TextArea appearance="standard" name="reason" required minLength={3} />
            </Field>
            <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy}>
              Record listing decision
            </Button>
          </FormLayout>
        ))}
      {!recordOnly && <FormLayout sourceVersion={sourceVersion} allowed={active}
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
      </FormLayout>}
      {data.carts
        .filter((x) => x.status === "open")
        .map((cart) => (
          <Card as="section" appearance="standard" className="ui-panel" key={String(cart.id)}>
            <h2>Cart · {String(cart.id)}</h2>
            <FormLayout sourceVersion={sourceVersion} allowed={active}
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
            <FormLayout sourceVersion={sourceVersion} allowed={active}
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
            <FormLayout sourceVersion={sourceVersion} allowed={purchase?.settleExisting!==false} editorTitle="Abandon open cart" className="ui-form-grid" onSubmit={event=>{event.preventDefault();void send({entity:'cart',id:cart.id,action:'abandon',expectedVersion:Number(cart.version),reason:String(new FormData(event.currentTarget).get('reason'))});}}>
              <p>Close this open cart and retain its original listing quantities and prices for history.</p><Field appearance="standard">Reason<TextArea appearance="standard" name="reason" required minLength={3} maxLength={2000}/></Field><Button appearance="standard" clientManaged type="submit" disabled={busy}>Abandon cart</Button>
            </FormLayout>
          </Card>
        ))}
    </Stack></div></CRMDraftBoundary></CRMCommandState>
  );
}
