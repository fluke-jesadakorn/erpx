"use client";
import {PortalSupportResponseEditor} from "@/components/customer_authority/portal-support-response-editor";
import { canonicalUiHref } from "@/lib/work_projection/destination-registry";
import {CRMForm as FormLayout,CRMCommandState,CRMCommandFeedback,CRMDraftBoundary,useCRMSnapshot} from '@/components/crm/draft-boundary';
import {useCRMCommand} from '@/components/crm/use-crm-command';

import { Button, Field, Select, TextArea, TextInput } from "@/components/ui/foundation";

import { TimeText } from "@/components/people_payroll/time/time-preference-context";
import { PreferredDateTimeField } from "@/components/people_payroll/time/preferred-date-time-field";
import { useRouter } from "@/lib/shared/destination-router";
import { hubResultHref } from "@/lib/shared/hub-result";
import type { customerAccessHistory, customerAccessWorkspace } from "@/server/modules/customer_authority/public/customer-access";
import { Card } from "@/components/ui/foundation";
export function CustomerAccessConsole({
  base,
  data:currentData,
  detail:currentDetail,
  selectedOrderId,
  requestFormInline=false,
}: {
  base: string;
  data?: Awaited<ReturnType<typeof customerAccessWorkspace>>;
  detail?: Awaited<ReturnType<typeof customerAccessHistory>>;
  selectedOrderId?: string;
  requestFormInline?: boolean;
}) {
  const router=useRouter(),{snapshot,rootProps}=useCRMSnapshot({data:currentData,detail:currentDetail}),{data,detail}=snapshot,purchase=currentData?.purchase??currentDetail?.purchase,active=purchase?.newWork!==false,settle=purchase?.settleExisting!==false,canReview=currentDetail?.binding.canReview!==false,canReject=currentDetail?.binding.canReject!==false;
  const sourceVersion=JSON.stringify([currentDetail?.binding.version,currentData?.sources.map(row=>[row.id,row.version])]),command=useCRMCommand(base,'customer-access',input=>{if(!input||typeof input!=='object')return false;const value=input as Record<string,unknown>;return !value.action?active:value.action==='approve'?active&&canReview:settle&&(value.action!=='reject'||canReject);},receipt=>{const href=canonicalUiHref(`${base}/customer-access/${receipt.id}`);router.push(hubResultHref(window.location.href,base,href)??href,{scroll:false});}),{send,busy}=command;
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
        </Card>
      )}
      {detail?.supportThreads?.map(thread=><PortalSupportResponseEditor key={thread.requestId} base={base} bindingVersion={Number(detail.binding.version)} thread={thread}/>)}
    </div></CRMDraftBoundary></CRMCommandState>
  );
}
