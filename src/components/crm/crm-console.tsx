"use client";
import { canonicalUiHref } from "@/lib/work_projection/destination-registry";
import {CRMEditor as HubForm,CRMForm as FormLayout,CRMDraftBoundary,CRMCommandState,CRMCommandFeedback,useCRMSnapshot} from "./draft-boundary";
import {useCRMCommand} from "./use-crm-command";
import {crmCommandAllowed} from "@/lib/crm/purchase-controls";

import { Button, Field, Select, TextArea, TextInput } from "@/components/ui/foundation";

import { useState } from "react";
import { useRouter } from "@/lib/shared/destination-router";
import { hubResultHref } from "@/lib/shared/hub-result";
import type { crmDetail, crmWorkspace } from "@/server/modules/crm/public/crm";
import type { crmIdentities } from "@/server/modules/crm/public/crm-identities";
import { Card } from "@/components/ui/foundation";
export function CRMConsole({
  base,
  data: currentData,
  identityData: currentIdentityData,
  detail: currentDetail,
  inlineCapture=false,
}: {
  base: string;
  inlineCapture?: boolean;
  data?: Awaited<ReturnType<typeof crmWorkspace>>;
  identityData?: Awaited<ReturnType<typeof crmIdentities>>;
  detail?: Awaited<ReturnType<typeof crmDetail>>;
}) {
  const {snapshot,rootProps}=useCRMSnapshot({data:currentData,identityData:currentIdentityData,detail:currentDetail}),{data,identityData,detail}=snapshot;
  const router=useRouter(),[selectedAccount,setSelectedAccount]=useState(''),r=detail?.row,purchase=currentData?.purchase??currentDetail?.purchase,references=currentData?.referencePurchases??currentDetail?.referencePurchases;
  const active=purchase?.newWork!==false,settle=purchase?.settleExisting!==false,hotelActive=active&&references?.hotel.newWork!==false,hotelSettle=settle&&references?.hotel.settleExisting!==false;
  const sourceVersion=JSON.stringify([currentDetail?.row.version,currentDetail?.row.opportunity_version,currentDetail?.quotes.map(row=>[row.id,row.version]),currentDetail?.sales.map(row=>[row.id,row.version]),currentIdentityData?.accounts.map(row=>[row.id,row.version]),currentIdentityData?.contacts.map(row=>[row.id,row.version]),currentData?.campaigns.map(row=>row.id)]);
  const command=useCRMCommand(base,'crm',input=>{const body=input as Record<string,unknown>,role=body.domain==='identity'?currentDetail?.canManage:!body.id?currentData?.canCreate:['reassign','reopen'].includes(String(body.action))?currentDetail?.canManage:currentDetail?.canAct;return role!==false&&crmCommandAllowed(purchase,input,references);},(receipt,input)=>{if(input.domain!=='identity'){const destination=canonicalUiHref(`${base}/crm/${receipt.id}`);router.push(hubResultHref(window.location.href,base,destination)??destination,{scroll:false});}}),{send,busy}=command;
  const reason = (
      <Field appearance="standard">
        Evidence and reason
        <TextArea appearance="standard" name="reason" minLength={3} maxLength={2000} required />
      </Field>
    ),
    button = (label: string) => (
      <Button appearance="standard" type="submit" clientManaged variant="primary" disabled={busy}>
        {label}
      </Button>
    ),
    lead = (f: FormData, action: string) => ({
      id: r!.id,
      action,
      expectedVersion: Number(r!.version),
      reason: f.get("reason"),
    }),
    deal = (f: FormData, action: string) => ({
      ...lead(f, action),
      domain: "opportunity",
      expectedVersion: Number(r!.opportunity_version),
      leadVersion: Number(r!.version),
    });
  return (
    <CRMCommandState {...command}><CRMDraftBoundary><div {...rootProps}>
      <CRMCommandFeedback/>
      {data?.canCreate && (
        <Card as="section" appearance="standard" className="ui-panel">
          <h2>Capture lead</h2>
          <FormLayout sourceVersion={sourceVersion} allowed={active&&currentData?.canCreate!==false} inline={inlineCapture}
            className="ui-form-grid"
            onSubmit={(e) => {
              e.preventDefault();
              const f = new FormData(e.currentTarget);
              void send({
                branchId: f.get("branch"),
                accountId: f.get("accountId") || undefined,
                contactId: f.get("contactId") || undefined,
                title: f.get("title"),
                contactName: f.get("contact"),
                contactDetail: f.get("contactDetail"),
                source: f.get("source"),
                sourceEvidence: f.get("sourceEvidence"),
                campaignId: f.get("campaignId") || undefined,
                brief: f.get("brief"),
                dueDate: f.get("due"),
              });
            }}
          >
            <Field appearance="standard">
              Branch
              <Select appearance="standard" name="branch" required>
                <option value="">Choose branch</option>
                {data.branches.map((b) => (
                  <option key={String(b.id)} value={String(b.id)}>
                    {String(b.display_name)}
                  </option>
                ))}
              </Select>
            </Field>
            <Field appearance="standard">
              Existing Account (optional)
              <Select appearance="standard"
                name="accountId"
                value={selectedAccount}
                onChange={(e) => setSelectedAccount(e.target.value)}
              >
                <option value="">New individual prospect</option>
                {identityData?.accounts.map((a) => (
                  <option key={String(a.id)} value={String(a.id)}>
                    {String(a.name)}
                  </option>
                ))}
              </Select>
            </Field>
            {selectedAccount && (
              <Field appearance="standard">
                Existing Contact (optional)
                <Select appearance="standard" name="contactId">
                  <option value="">Create contact in this Account</option>
                  {identityData?.contacts
                    .filter((c) => c.account_id === selectedAccount)
                    .map((c) => (
                      <option key={String(c.id)} value={String(c.id)}>
                        {String(c.name)}
                      </option>
                    ))}
                </Select>
              </Field>
            )}
            <Field appearance="standard">
              Inquiry title
              <TextInput appearance="standard" name="title" minLength={3} maxLength={160} required />
            </Field>
            <Field appearance="standard">
              Contact name
              <TextInput appearance="standard" name="contact" minLength={2} maxLength={160} required />
            </Field>
            <Field appearance="standard">
              Contact details and requested channel
              <TextArea appearance="standard" name="contactDetail" minLength={3} maxLength={2000} required />
            </Field>
            <Field appearance="standard">
              Inquiry source
              <TextInput appearance="standard" name="source" minLength={3} maxLength={2000} required />
            </Field>
            <Field appearance="standard">
              Source evidence
              <TextArea appearance="standard" name="sourceEvidence" minLength={3} maxLength={2000} required />
            </Field>
            <Field appearance="standard">
              Campaign ID
              <Select appearance="standard" name="campaignId">
                <option value="">No linked campaign</option>
                {data.campaigns.map((c) => (
                  <option key={String(c.id)} value={String(c.id)}>
                    {String(c.code)} · {String(c.name)}
                  </option>
                ))}
              </Select>
            </Field>
            <Field appearance="standard">
              Customer needs and conditions
              <TextArea appearance="standard" name="brief" minLength={3} maxLength={2000} required />
            </Field>
            <Field appearance="standard">
              Follow-up due date
              <TextInput appearance="standard" name="due" type="date" required />
            </Field>
            {button("Create lead assigned to me")}
          </FormLayout>
        </Card>
      )}
      {r && detail?.canAct && r.status === "open" && (
        <Card as="section" appearance="standard" className="ui-panel">
          <h2>Qualify customer needs</h2>
          <FormLayout sourceVersion={sourceVersion} allowed={active&&currentDetail?.canAct!==false}
            className="ui-form-grid"
            onSubmit={(e) => {
              e.preventDefault();
              const f = new FormData(e.currentTarget);
              void send({
                ...lead(f, "qualify"),
                customerId: f.get("customer"),
                expectedValue: f.get("value"),
                closeDate: f.get("close"),
              });
            }}
          >
            <p>
              {r.account_customer_id
                ? "Finance customer link has been reviewed."
                : "A separate reviewer must link this Account to a Finance customer first."}
            </p>
            <Field appearance="standard">
              Reviewed customer identity
              <Select appearance="standard" name="customer" required>
                <option value="">Select the existing customer</option>
                {detail.customers
                  .filter((c) => c.id === r.account_customer_id)
                  .map((c) => (
                    <option key={String(c.id)} value={String(c.id)}>
                      {String(c.name)}
                    </option>
                  ))}
              </Select>
            </Field>
            <Field appearance="standard">
              Expected net value ({detail.companyCurrency})
              <TextInput appearance="standard" name="value" type="number" min="0" step="0.01" required />
            </Field>
            <Field appearance="standard">
              Expected close date
              <TextInput appearance="standard" name="close" type="date" required />
            </Field>
            {reason}
            {button("Qualify and create opportunity")}
          </FormLayout>
          <FormLayout sourceVersion={sourceVersion} allowed={settle&&currentDetail?.canAct!==false}
            className="ui-form-grid"
            onSubmit={(e) => {
              e.preventDefault();
              void send(lead(new FormData(e.currentTarget), "disqualify"));
            }}
          >
            {reason}
            {button("Disqualify lead")}
          </FormLayout>
        </Card>
      )}
      {r && detail?.canManage && r.stage !== "won" && r.status !== "disqualified" && (
        <Card as="section" appearance="standard" className="ui-panel">
          <h2>Change responsibility</h2>
          <FormLayout sourceVersion={sourceVersion} allowed={settle&&currentDetail?.canManage!==false}
            className="ui-form-grid"
            onSubmit={(e) => {
              e.preventDefault();
              const f = new FormData(e.currentTarget);
              void send({ ...lead(f, "reassign"), ownerId: f.get("owner"), dueDate: f.get("due") });
            }}
          >
            <Field appearance="standard">
              Eligible salesperson
              <Select appearance="standard" name="owner" required defaultValue={String(r.owner_id)}>
                <option value="">Choose colleague</option>
                {detail.recipients.map((p) => (
                  <option key={p.id} value={p.id}>
                    {p.name}
                  </option>
                ))}
              </Select>
            </Field>
            <Field appearance="standard">
              Follow-up due date
              <TextInput appearance="standard" name="due" type="date" defaultValue={String(r.due_date)} required />
            </Field>
            {reason}
            {button("Record reassignment")}
          </FormLayout>
        </Card>
      )}
      {r && detail?.canManage && r.status === "disqualified" && (
        <FormLayout sourceVersion={sourceVersion} allowed={active&&currentDetail?.canManage!==false}
          className="ui-panel ui-form-grid"
          onSubmit={(e) => {
            e.preventDefault();
            void send(lead(new FormData(e.currentTarget), "reopen"));
          }}
        >
          <h2>Reopen lead</h2>
          {reason}
          {button("Reopen with reason")}
        </FormLayout>
      )}
      {r &&
        detail?.canAct &&
        r.opportunity_id &&
        ["discovery", "proposal", "negotiation"].includes(String(r.stage)) && (
          <Card as="section" appearance="standard" className="ui-panel">
            <h2>Commercial handoff</h2>
            <p>
              Prepare the quotation in Sales for this customer and branch. Another manager must
              approve it before linking it here.
            </p>
            <FormLayout sourceVersion={sourceVersion} allowed={settle&&currentDetail?.canAct!==false}
              className="ui-form-grid"
              onSubmit={(e) => {
                e.preventDefault();
                const f = new FormData(e.currentTarget),
                  quote = detail.quotes.find((q) => q.id === f.get("quote"));
                if (quote)
                  void send({
                    ...deal(f, "link-quote"),
                    quoteId: quote.id,
                    quoteVersion: Number(quote.version),
                  });
              }}
            >
              <Field appearance="standard">
                Approved quotation
                <Select appearance="standard" name="quote" required>
                  <option value="">Choose approved quotation</option>
                  {detail.quotes.map((q) => (
                    <option key={String(q.id)} value={String(q.id)}>
                      {String(q.number)}
                    </option>
                  ))}
                </Select>
              </Field>
              {reason}
              {button("Link approved quotation")}
            </FormLayout>
            {r.quote_id && (
              <FormLayout sourceVersion={sourceVersion} allowed={settle&&currentDetail?.canAct!==false}
                className="ui-form-grid"
                onSubmit={(e) => {
                  e.preventDefault();
                  void send(deal(new FormData(e.currentTarget), "negotiate"));
                }}
              >
                {reason}
                {button("Record negotiation")}
              </FormLayout>
            )}
            <FormLayout sourceVersion={sourceVersion} allowed={settle&&currentDetail?.canAct!==false}
              className="ui-form-grid"
              onSubmit={(e) => {
                e.preventDefault();
                const f = new FormData(e.currentTarget),
                  sale = detail.sales.find((s) => s.id === f.get("sale"));
                if (sale)
                  void send({
                    ...deal(f, "win"),
                    salesId: sale.id,
                    salesVersion: Number(sale.version),
                  });
              }}
            >
              <Field appearance="standard">
                Confirmed sales order converted from quotation
                <Select appearance="standard" name="sale" required>
                  <option value="">Choose confirmed sales order</option>
                  {detail.sales.map((s) => (
                    <option key={String(s.id)} value={String(s.id)}>
                      {String(s.number)}
                    </option>
                  ))}
                </Select>
              </Field>
              {reason}
              {button("Record won opportunity")}
            </FormLayout>
            <FormLayout sourceVersion={sourceVersion} allowed={settle&&currentDetail?.canAct!==false}
              className="ui-form-grid"
              onSubmit={(e) => {
                e.preventDefault();
                const f = new FormData(e.currentTarget);
                void send({ ...deal(f, "lose"), lossReason: f.get("lossReason") });
              }}
            >
              <Field appearance="standard">
                Loss reason
                <Select appearance="standard" name="lossReason">
                  <option value="price">Price</option>
                  <option value="availability">Availability</option>
                  <option value="competitor">Competitor</option>
                  <option value="timing">Timing</option>
                  <option value="no_response">No response</option>
                  <option value="other">Other</option>
                </Select>
              </Field>
              {reason}
              {button("Record lost opportunity")}
            </FormLayout>
          </Card>
        )}
      {r && detail?.canManage && r.contact_id && detail.guestCandidates.length > 0 && (
        <HubForm disabled={!hotelActive} title="Review PMS guest link" label="Review PMS guest link" primary={false}><FormLayout sourceVersion={sourceVersion} allowed={hotelActive&&currentDetail?.canManage!==false}
          className="ui-panel ui-form-grid"
          onSubmit={(e) => {
            e.preventDefault();
            const f = new FormData(e.currentTarget),
              guest = detail.guestCandidates.find((g) => g.id === f.get("guestId"));
            if (guest)
              void send({
                domain: "identity",
                action: "link-guest",
                contactId: r.contact_id,
                guestId: guest.id,
                propertyId: guest.property_id,
                evidence: f.get("evidence"),
                sharingEvidence:f.get("sharingEvidence")||undefined,
              });
          }}
        >
          <h2>Review PMS guest link</h2>
          <Field appearance="standard">
            PMS guest
            <Select appearance="standard" name="guestId" required>
              <option value="">Select guest</option>
              {detail.guestCandidates.map((g) => (
                <option key={String(g.id)} value={String(g.id)}>
                  {String(g.name)} · {String(g.property_name)}
                </option>
              ))}
            </Select>
          </Field>
          <Field appearance="standard">
            Identity review evidence
            <TextArea appearance="standard" name="evidence" required minLength={3} />
          </Field>
          <Field appearance="standard">Guest service-history authorization evidence (optional)<TextArea appearance="standard" name="sharingEvidence" minLength={3}/></Field><p>Without authority evidence, the identity link is retained but hotel history stays private.</p>
          {button("Confirm guest link")}
        </FormLayout></HubForm>
      )}
      {r &&
        detail?.canAct &&
        r.opportunity_id &&
        ["discovery", "proposal", "negotiation"].includes(String(r.stage)) &&
        detail.reservations.some((x) => x.state === "confirmed") && (
          <FormLayout sourceVersion={sourceVersion} allowed={hotelSettle&&currentDetail?.canAct!==false}
            className="ui-panel ui-form-grid"
            onSubmit={(e) => {
              e.preventDefault();
              const f = new FormData(e.currentTarget),
                reservation = detail.reservations.find((x) => x.id === f.get("reservationId"));
              if (reservation)
                void send({
                  ...deal(f, "win-pms"),
                  reservationId: reservation.id,
                  propertyId: reservation.property_id,
                });
            }}
          >
            <h2>Confirm hotel outcome</h2>
            <Field appearance="standard">
              Confirmed PMS reservation
              <Select appearance="standard" name="reservationId" required>
                <option value="">Select reservation</option>
                {detail.reservations
                  .filter((x) => x.state === "confirmed")
                  .map((x) => (
                    <option key={String(x.id)} value={String(x.id)}>
                      {String(x.number)}
                    </option>
                  ))}
              </Select>
            </Field>
            {reason}
            {button("Record won from confirmed reservation")}
          </FormLayout>
        )}
      {r && detail?.canManage && r.stage === "lost" && (
        <FormLayout sourceVersion={sourceVersion} allowed={active&&currentDetail?.canManage!==false}
          className="ui-panel ui-form-grid"
          onSubmit={(e) => {
            e.preventDefault();
            void send(deal(new FormData(e.currentTarget), "reopen"));
          }}
        >
          <h2>Reopen opportunity</h2>
          <p>
            The previous quotation remains in history; approve and link a current quotation again.
          </p>
          {reason}
          {button("Reopen opportunity")}
        </FormLayout>
      )}
    </div></CRMDraftBoundary></CRMCommandState>
  );
}
