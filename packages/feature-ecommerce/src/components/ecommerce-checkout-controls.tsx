"use client";
import {CRMForm as FormLayout,CRMCommandState,CRMCommandFeedback,CRMDraftBoundary,useCRMSnapshot} from '@erpx/feature-crm/components/draft-boundary';
import {useCRMCommand} from '@erpx/feature-crm/components/use-crm-command';
import {ecommerceCommandAllowed} from '@erpx/feature-ecommerce/lib/purchase-controls';

import { Button, Field, Select, TextArea, TextInput } from "@erpx/ui/components/foundation";

import type { ecommerceDetail } from "@erpx/server-ecommerce/public/ecommerce";
import { Card } from "@erpx/ui/components/foundation";

export function EcommerceCheckoutControls({
  base,
  data:currentData,
}: {
  base: string;
  data: Awaited<ReturnType<typeof ecommerceDetail>>;
}) {
  const {snapshot:data,rootProps}=useCRMSnapshot(currentData),purchase=currentData.purchase,command=useCRMCommand(base,'ecommerce',input=>ecommerceCommandAllowed(purchase,input)),{send,busy}=command,sourceVersion=JSON.stringify([currentData.checkout.version,currentData.order.fulfillments.map(row=>[row.id,row.version])]);
  const checkout = data.checkout;
  return (
    <CRMCommandState {...command}><CRMDraftBoundary><div {...rootProps}><Card as="section" appearance="standard" className="ui-panel ui-form-grid">
      <h2>Owner actions</h2>
      <CRMCommandFeedback/>
      {checkout.status === "submitted" && (
        <FormLayout sourceVersion={sourceVersion} allowed={purchase?.newWork!==false&&purchase?.canReview!==false}
          onSubmit={(event) => {
            event.preventDefault();
            void send({
              entity: "checkout",
              id: checkout.id,
              action: "approve-order",
              expectedVersion: Number(checkout.version),
              reason: String(new FormData(event.currentTarget).get("reason")),
            });
          }}
        >
          <Field appearance="standard">
            Approval reason
            <TextArea appearance="standard" name="reason" required minLength={3} />
          </Field>
          <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy}>
            Approve order and reserve stock
          </Button>
        </FormLayout>
      )}
      {(checkout.status !== "submitted"||purchase?.newWork===false) && (
        <FormLayout sourceVersion={sourceVersion} allowed={purchase?.settleExisting!==false}
          onSubmit={(event) => {
            event.preventDefault();
            void send({
              entity: "checkout",
              id: checkout.id,
              action: "sync-fulfillment",
              expectedVersion: Number(checkout.version),
              reason: String(new FormData(event.currentTarget).get("reason")),
            });
          }}
        >
          <Field appearance="standard">
            Sync evidence
            <TextArea appearance="standard" name="reason" required minLength={3} />
          </Field>
          <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy}>
            Sync Sales fulfillment
          </Button>
        </FormLayout>
      )}
      {data.order.fulfillments.flatMap((fulfillment) =>
        data.order.fulfillmentLines
          .filter((line) => line.fulfillment_id === fulfillment.id)
          .map((line) => (
            <FormLayout sourceVersion={sourceVersion} allowed={purchase?.settleExisting!==false}
              key={String(line.id)}
              onSubmit={(event) => {
                event.preventDefault();
                const form = new FormData(event.currentTarget);
                void send({
                  entity: "checkout",
                  id: checkout.id,
                  action: "request-return",
                  expectedVersion: Number(checkout.version),
                  reason: String(form.get("reason")),
                  fulfillmentId: fulfillment.id,
                  fulfillmentVersion: Number(fulfillment.version),
                  date: String(form.get("date")),
                  lines: [{ lineId: line.id, quantity: String(form.get("quantity")),condition:String(form.get("condition")),conditionNote:String(form.get("conditionNote")),qualityDueDate:String(form.get("qualityDueDate")),
                    ...(data.order.lotAllocations.some(allocation=>allocation.fulfillment_line_id===line.id)?{allocations:data.order.lotAllocations.filter(allocation=>allocation.fulfillment_line_id===line.id&&Number(form.get(`lot-${allocation.id}`))>0).map(allocation=>({lotId:String(allocation.lot_id),quantity:String(form.get(`lot-${allocation.id}`))}))}:{}),
                  }],
                });
              }}
            >
              <h3>Return delivery {String(fulfillment.number)}</h3>
              <Field appearance="standard">
                Date
                <TextInput appearance="standard" name="date" type="date" required />
              </Field>
              <Field appearance="standard">
                Quantity
                <TextInput appearance="standard" name="quantity" defaultValue="1" required />
              </Field>
              {data.order.lotAllocations.filter(allocation=>allocation.fulfillment_line_id===line.id&&Number(allocation.quantity_milli)>Number(allocation.returned_milli)).map(allocation=><Field appearance="standard" key={String(allocation.id)}>
                Return from {String(allocation.lot_code??allocation.serial_number)}
                <TextInput appearance="standard" name={`lot-${allocation.id}`} type="number" min="0" max={(Number(allocation.quantity_milli)-Number(allocation.returned_milli))/1000} step={allocation.serial_number?'1':'0.001'} defaultValue="0"/>
              </Field>)}
              <Field appearance="standard">Returned condition<Select appearance="standard" name="condition" defaultValue="unknown"><option value="unknown">Needs inspection</option><option value="unopened">Unopened</option><option value="damaged">Damaged</option></Select></Field>
              <Field appearance="standard">Condition evidence<TextArea appearance="standard" name="conditionNote" required minLength={3} maxLength={500}/></Field>
              <Field appearance="standard">Quality review due<TextInput appearance="standard" name="qualityDueDate" type="date" min={String(fulfillment.fulfillment_date)} required/></Field>
              <Field appearance="standard">
                Reason
                <TextArea appearance="standard" name="reason" required minLength={3} />
              </Field>
              <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy}>
                Request return
              </Button>
            </FormLayout>
          )),
      )}
    </Card></div></CRMDraftBoundary></CRMCommandState>
  );
}
