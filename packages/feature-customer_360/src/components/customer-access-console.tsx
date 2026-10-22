"use client";
import {PortalSupportResponseEditor} from "@erpx/feature-customer_authority/components/portal-support-response-editor";
import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";
import {CRMForm as FormLayout,CRMCommandState,CRMCommandFeedback,CRMDraftBoundary,useCRMSnapshot} from '@erpx/feature-crm/components/draft-boundary';
import {useCRMCommand} from '@erpx/feature-crm/components/use-crm-command';

import { Button, Field, Select, TextArea, TextInput } from "@erpx/ui/components/foundation";

import { TimeText } from "@erpx/feature-people_payroll/components/time/time-preference-context";
import { PreferredDateTimeField } from "@erpx/feature-people_payroll/components/time/preferred-date-time-field";
import { useRouter } from "@erpx/feature-shared/lib/destination-router";
import { hubResultHref } from "@erpx/feature-shared/lib/hub-result";
import type { customerAccessHistory, customerAccessWorkspace } from "@erpx/server-customer_authority/public/customer-access";
import { Card } from "@erpx/ui/components/foundation";
type CustomerAccessDetail = Awaited<ReturnType<typeof customerAccessHistory>> & {
  invitations?: Record<string, unknown>[];
  invitationEvents?: Record<string, unknown>[];
};
export function CustomerAccessConsole({
  base,
  data:currentData,
  detail:currentDetail,
  selectedOrderId,
  requestFormInline=false,
}: {
  base: string;
  data?: Awaited<ReturnType<typeof customerAccessWorkspace>>;
  detail?: CustomerAccessDetail;
  selectedOrderId?: string;
  requestFormInline?: boolean;
}) {
  const router=useRouter(),{snapshot,rootProps}=useCRMSnapshot({data:currentData,detail:currentDetail}),{data,detail}=snapshot,purchase=currentData?.purchase??currentDetail?.purchase,active=purchase?.newWork!==false,settle=purchase?.settleExisting!==false,canReview=currentDetail?.binding.canReview!==false,canReject=currentDetail?.binding.canReject!==false,canInvite=currentDetail?.binding.canInvite===true,activeInvitation=detail?.invitations?.find(invite=>invite.isActive===true);
  const sourceVersion=JSON.stringify([currentDetail?.binding.version,currentData?.sources.map(row=>[row.id,row.version]),detail?.invitations?.map(invite=>[invite.id,invite.generation,invite.state,invite.isActive])]),command=useCRMCommand(base,'customer-access',input=>{if(!input||typeof input!=='object')return false;const value=input as Record<string,unknown>;return !value.action?active:value.action==='approve'?active&&canReview:value.action==='invite'?active&&canInvite:value.action==='resend'?settle&&canInvite:settle&&(value.action!=='reject'||canReject);},receipt=>{const href=canonicalUiHref(`${base}/customer-access/${receipt.id}`);router.push(hubResultHref(window.location.href,base,href)??href,{scroll:false});}),{send,busy}=command;
  const reason = (
    <Field appearance="standard">
      Authority evidence and reason
      <TextArea appearance="standard" name="reason" minLength={3} maxLength={2000} required />
    </Field>
  );
  return (
    <CRMCommandState {...command}><CRMDraftBoundary><div {...rootProps}>
      <CRMCommandFeedback/>
      {data && (
        <Card as="section" appearance="standard" className="ui-panel">
          <h2>Request source-bound customer access</h2>
          <FormLayout inline={requestFormInline} sourceVersion={sourceVersion} allowed={active}
            className="ui-form-grid"
            onSubmit={(e) => {
              e.preventDefault();
              const f = new FormData(e.currentTarget),
                source = data.sources.find((s) => s.id === f.get("orderId"));
              if (source)
                void send({
                  orderId: source.id,
                  sourceVersion: Number(source.version),
                  email: f.get("email"),
                  expiresAt: new Date(String(f.get("expiresAt"))).getTime(),
                  reason: f.get("reason"),
                });
            }}
          >
            <Field appearance="standard">
              Approved customer document
              <Select appearance="standard" name="orderId" defaultValue={selectedOrderId ?? ""} required>
                <option value="">Choose source</option>
                {data.sources.map((s) => (
                  <option key={String(s.id)} value={String(s.id)}>
                    {String(s.name)} · {String(s.number)}
                  </option>
                ))}
              </Select>
            </Field>
            <Field appearance="standard">
              Verified customer email
              <TextInput appearance="standard" type="email" name="email" autoComplete="off" required />
            </Field>
            <PreferredDateTimeField appearance="standard" name="expiresAt" label="Access expires" required />
            {reason}
            <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy}>
              Send for independent review
            </Button>
          </FormLayout>
        </Card>
      )}
      {detail && (
        <Card as="section" appearance="standard" className="ui-panel">
          <h2>Access decision</h2>
          <p>
            {String(detail.binding.email)} · {String(detail.binding.status)} · expires{" "}
            <TimeText appearance="standard" value={Number(detail.binding.expires_at)} />
          </p>
          {Boolean(detail.binding.canReview||detail.binding.canReject) && (
            <FormLayout sourceVersion={sourceVersion} allowed={canReview&&active||canReject&&settle}
              className="ui-form-grid"
              onSubmit={(e) => {
                e.preventDefault();
                const f = new FormData(e.currentTarget);
                void send({
                  id: detail.binding.id,
                  action: f.get("action"),
                  expectedVersion: Number(detail.binding.version),
                  reason: f.get("reason"),
                });
              }}
            >
              <Field appearance="standard">
                Decision
                <Select appearance="standard" name="action" defaultValue={detail.binding.canReview?'approve':'reject'}>
                  <option value="approve" disabled={!active||!canReview}>Approve access</option>
                  <option value="reject" disabled={!settle||!canReject}>Reject request</option>
                </Select>
              </Field>
              {reason}
              <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy}>
                Record independent decision
              </Button>
            </FormLayout>
          )}
          {Boolean(detail.binding.canRevoke) &&
            ["pending", "approved"].includes(String(detail.binding.status)) && (
              <FormLayout sourceVersion={sourceVersion} allowed={settle}
                className="ui-form-grid"
                onSubmit={(e) => {
                  e.preventDefault();
                  const f = new FormData(e.currentTarget);
                  void send({
                    id: detail.binding.id,
                    action: "revoke",
                    expectedVersion: Number(detail.binding.version),
                    reason: f.get("reason"),
                  });
                }}
              >
                {reason}
                <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy}>
                  Revoke access
                </Button>
              </FormLayout>
            )}
          {canInvite&&detail.binding.status==='approved'&&<>
            <h3>Customer Portal invitation</h3>
            {activeInvitation&&<p>Invitation {String(activeInvitation.state)} · generation {String(activeInvitation.generation)} · expires <TimeText appearance="standard" value={Number(activeInvitation.token_expires_at)}/>{activeInvitation.sent_at?<> · sent <TimeText appearance="standard" value={Number(activeInvitation.sent_at)}/></>:null}</p>}
            <FormLayout sourceVersion={sourceVersion} allowed={activeInvitation?settle:active} className="ui-form-grid" onSubmit={event=>{event.preventDefault();const form=new FormData(event.currentTarget);void send({id:detail.binding.id,action:activeInvitation?'resend':'invite',expectedVersion:Number(detail.binding.version),reason:form.get('reason')});}}>
              {reason}
              <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy || (activeInvitation ? !settle : !active)}>
                {activeInvitation?'Send a fresh invitation link':'Invite verified customer to the Portal'}
              </Button>
            </FormLayout>
          </>}
          {detail.invitations && detail.invitations.length > 0 && <div><h3>Invitation history</h3>{detail.invitations.map(invite=><p key={String(invite.id)}>Generation {String(invite.generation)} · {String(invite.state)} · <TimeText appearance="standard" value={Number(invite.created_at)}/></p>)}</div>}
        </Card>
      )}
      {detail?.supportThreads?.map(thread=><PortalSupportResponseEditor key={thread.requestId} base={base} bindingVersion={Number(detail.binding.version)} thread={thread}/>)}
    </div></CRMDraftBoundary></CRMCommandState>
  );
}
