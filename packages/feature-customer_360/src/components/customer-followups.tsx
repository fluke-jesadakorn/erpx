"use client";
import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";

import { Button, Field, Select, TextArea, TextInput } from "@erpx/ui/components/foundation";

import {CRMCommandFeedback,CRMCommandState,CRMDraftBoundary,CRMForm,useCRMSnapshot} from "@erpx/feature-crm/components/draft-boundary";
import {useCRMCommand} from "@erpx/feature-crm/components/use-crm-command";
import type {FollowupPurchaseControls} from "@erpx/feature-sales/lib/followup-purchases";
import { useRouter } from "@erpx/feature-shared/lib/destination-router";
import { hubResultHref } from "@erpx/feature-shared/lib/hub-result";
import type { Row } from "@erpx/server-commerce/public/commerce";
import type { customerFollowupDetail } from "@erpx/server-sales/public/customer-followups";
import { Card } from "@erpx/ui/components/foundation";
type FollowupProps={base:string;order?:Row|null;recipients:{id:string;name:string}[];detail?:Awaited<ReturnType<typeof customerFollowupDetail>>;purchase?:FollowupPurchaseControls;inlineCreate?:boolean};
export function CustomerFollowupForm(props:FollowupProps){
 const captured=useCRMSnapshot(props),{base,order,recipients,detail,inlineCreate}=captured.snapshot,router=useRouter(),purchase=props.purchase??props.detail?.purchase;
 const allowed=(input:unknown)=>{if(!input||typeof input!=='object')return false;const value=input as {id?:unknown;action?:unknown};return value.id?(!purchase||purchase.settleExisting)&&(value.action==='complete'?!!props.detail?.canComplete:value.action==='reassign'?!!props.detail?.canManage:value.action==='cancel'?!!props.detail?.canComplete||!!props.detail?.canManage:false):(!purchase||purchase.newWork);};
 const command=useCRMCommand(base,'customer-followups',allowed,receipt=>{const destination=canonicalUiHref(`${base}/customer-followups/${receipt.id}`);router.push(hubResultHref(window.location.href,base,destination)??destination,{scroll:false});}),{send,busy}=command;
 const version=props.detail?.row.version??props.order?.version;
  const owners = (
    <Field appearance="standard">
      Responsible colleague
      <Select appearance="standard" name="owner" required defaultValue={String(detail?.row.owner_id ?? "")}>
        <option value="">Choose an eligible colleague</option>
        {recipients.map((p) => (
          <option key={p.id} value={p.id}>
            {p.name}
          </option>
        ))}
      </Select>
    </Field>
  );
  return (
    <CRMDraftBoundary><div {...captured.rootProps}><CRMCommandState {...command}><CRMCommandFeedback/>
      {order && (
        <Card as="section" appearance="standard" className="ui-panel">
          <h2>Schedule customer follow-up</h2>
          <CRMForm inline={inlineCreate} allowed={!purchase||purchase.newWork} editorTitle="Schedule customer follow-up"
            className="ui-form-grid"
            sourceVersion={typeof version==='number'||typeof version==='string'?version:undefined}
            onSubmit={(e) => {
              e.preventDefault();
              const f = new FormData(e.currentTarget);
              void send({
                orderId: order.id,
                expectedVersion: Number(order.version),
                ownerId: f.get("owner"),
                dueDate: f.get("due"),
                nextAction: f.get("next"),
              });
            }}
          >
            {owners}
            <Field appearance="standard">
              Due date
              <TextInput appearance="standard" name="due" type="date" required />
            </Field>
            <Field appearance="standard">
              Next action
              <TextArea appearance="standard" name="next" minLength={3} maxLength={2000} required />
            </Field>
            <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy || !recipients.length}>
              Assign follow-up
            </Button>
          </CRMForm>
          {!recipients.length && (
            <p>
              No eligible sales colleague. Ask the Company administrator to restore role coverage.
            </p>
          )}
        </Card>
      )}
      {detail?.canComplete && (
        <Card as="section" appearance="standard" className="ui-panel">
          <h2>Record contact outcome</h2>
          <p>
            This records what you did; it does not send an email or change the customer’s order.
          </p>
          <CRMForm allowed={(!purchase||purchase.settleExisting)&&!!props.detail?.canComplete} editorTitle="Record contact outcome"
            className="ui-form-grid"
            sourceVersion={typeof version==='number'||typeof version==='string'?version:undefined}
            onSubmit={(e) => {
              e.preventDefault();
              const f = new FormData(e.currentTarget);
              void send({
                id: detail.row.id,
                expectedVersion: Number(detail.row.version),
                action: f.get("action"),
                reason: f.get("reason"),
              });
            }}
          >
            <Field appearance="standard">
              Result
              <Select appearance="standard" name="action">
                <option value="complete">Contact completed</option>
                <option value="cancel">Cancel follow-up</option>
              </Select>
            </Field>
            <Field appearance="standard">
              Evidence and outcome
              <TextArea appearance="standard" name="reason" minLength={3} maxLength={2000} required />
            </Field>
            <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy}>
              Record outcome
            </Button>
          </CRMForm>
        </Card>
      )}
      {detail?.canManage && (
        <Card as="section" appearance="standard" className="ui-panel">
          <h2>Manage responsibility</h2>
          <CRMForm allowed={(!purchase||purchase.settleExisting)&&!!props.detail?.canManage} editorTitle="Manage responsibility"
            className="ui-form-grid"
            sourceVersion={typeof version==='number'||typeof version==='string'?version:undefined}
            onSubmit={(e) => {
              e.preventDefault();
              const f = new FormData(e.currentTarget);
              void send({
                id: detail.row.id,
                expectedVersion: Number(detail.row.version),
                action: "reassign",
                ownerId: f.get("owner"),
                dueDate: f.get("due"),
                reason: f.get("reason"),
              });
            }}
          >
            {owners}
            <Field appearance="standard">
              Due date
              <TextInput appearance="standard"
                name="due"
                type="date"
                defaultValue={String(detail.row.due_date)}
                required
              />
            </Field>
            <Field appearance="standard">
              Reason
              <TextArea appearance="standard" name="reason" minLength={3} maxLength={2000} required />
            </Field>
            <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy}>
              Record management decision
            </Button>
          </CRMForm>
          <CRMForm allowed={(!purchase||purchase.settleExisting)&&!!props.detail?.canManage} editorTitle="Cancel customer follow-up"
            className="ui-form-grid"
            sourceVersion={typeof version==='number'||typeof version==='string'?version:undefined}
            onSubmit={(e) => {
              e.preventDefault();
              void send({
                id: detail.row.id,
                expectedVersion: Number(detail.row.version),
                action: "cancel",
                reason: new FormData(e.currentTarget).get("reason"),
              });
            }}
          >
            <Field appearance="standard">
              Cancellation reason
              <TextArea appearance="standard" name="reason" minLength={3} maxLength={2000} required />
            </Field>
            <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy}>
              Cancel this follow-up
            </Button>
          </CRMForm>
        </Card>
      )}
    </CRMCommandState></div></CRMDraftBoundary>
  );
}
