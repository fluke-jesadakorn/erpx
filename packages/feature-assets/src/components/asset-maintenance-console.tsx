"use client";
import WorkspaceLink from "@erpx/workplace/components/workspace-link";
import { InlineNotice } from "@erpx/ui/components/foundation";

import {AssetEditor,AssetRecoveryBoundary} from "@erpx/feature-assets/components/asset-editor";
import {useAssetCommand} from "@erpx/feature-assets/components/use-asset-command";
import {assetCommandAllowed} from "@erpx/feature-assets/lib/command";
import { Button, Field, Select, TextArea, TextInput } from "@erpx/ui/components/foundation";
import { PreferredDateTimeField } from "@erpx/feature-people_payroll/components/time/preferred-date-time-field";
import { ServiceSpareForm } from "@erpx/feature-field_service/components/service-spare-form";
import type {assetDetail,assetWorkspace} from "@erpx/server-assets/public/asset-maintenance";
import type {maintenanceDetail,maintenanceWorkspace} from "@erpx/server-maintenance/public/asset-maintenance";
import { FormLayout } from "@erpx/ui/components/foundation";
import {useAccountPreferences} from "@erpx/feature-identity_access/components/account-preferences-context";
const retirementReviewCopy={
 en:{title:"Review updated retirement request",description:"Review the current asset and linked Finance source before renewing your retirement request. Enter the current reason and evidence. A different authorized reviewer must approve the renewed request."},
 th:{title:"ทบทวนคำขอเลิกใช้",description:"ทบทวนทรัพย์สินปัจจุบันและข้อมูล Finance ที่เชื่อมโยงก่อนยืนยันคำขอเลิกใช้อีกครั้ง ระบุเหตุผลและหลักฐานปัจจุบัน โดยผู้ตรวจที่ได้รับสิทธิ์คนอื่นต้องเป็นผู้อนุมัติคำขอที่ทบทวนแล้ว"},
};
export function AssetConsole({
  base,
  data,
  detail,
}: {
  base: string;
  data?: Awaited<ReturnType<typeof assetWorkspace>>;
  detail?: Awaited<ReturnType<typeof assetDetail>>;
}) {
  const language=useAccountPreferences()?.preference?.language,retirementReview=language==="th"?retirementReviewCopy.th:retirementReviewCopy.en;
  const purchases=detail?.purchases??data?.purchases,{send,retry,busy,error,notice,saved,uncertain}=useAssetCommand(base,"assets",purchases,detail?.asset),
    r = detail?.asset,
    retirementRequest = detail?.history.findLast(event => event.action === "retire-request");
  function action(name: string, title: string, extra?: React.ReactNode,available=true) {
    return (
      <AssetEditor key={name} title={title} busy={busy} error={error} visible={available} disabled={detail?.canCreate===false||!assetCommandAllowed("assets",{id:r?.id,action:name},purchases)}><FormLayout
        className="ui-panel ui-form-grid"
        onSubmit={(e) => {
          e.preventDefault();
          void send({
            ...Object.fromEntries(new FormData(e.currentTarget)),
            ...(name === "link-finance" ? {financeAssetVersion: Number(new FormData(e.currentTarget).get("financeAssetVersion"))} : {}),
            id: r!.id,
            action: name,
            expectedVersion: Number(r!.version),
          },e.currentTarget);
        }}
      >
        <h2>{title}</h2>
        {extra}
        <Field appearance="standard">
          Reason and evidence
          <TextArea appearance="standard" name="reason" required minLength={3} />
        </Field>
        <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy}>
          {title}
        </Button>
      </FormLayout></AssetEditor>
    );
  }
  return (
    <AssetRecoveryBoundary recovery={{retry,busy,uncertain,notice,saved}}>
      {(data?.canLinkFinance||detail?.canLinkFinance)&&<p><WorkspaceLink href={`${base}/services/assets/accounting`}>{language==="th"?"เตรียมและตรวจสอบการรับรู้หรือจำหน่ายทรัพย์สิน":"Prepare and review asset acquisition or disposal"}</WorkspaceLink></p>}
      {error && <InlineNotice tone="danger" role="alert">{error}</InlineNotice>}{notice&&<InlineNotice role="status">{notice}</InlineNotice>}{saved&&<p>Saved original: <WorkspaceLink href={saved.href}>{saved.id}</WorkspaceLink></p>}{uncertain&&<Button appearance="standard" type="button" disabled={busy} onClick={()=>void retry()}>Retry submitted change</Button>}
      {data && (
        <AssetEditor title="Register asset" visible={data.canCreate} busy={busy} error={error} disabled={!assetCommandAllowed("assets",{},purchases)}><FormLayout
          className="ui-panel ui-form-grid"
          onSubmit={(e) => {
            e.preventDefault();
            const f = Object.fromEntries(new FormData(e.currentTarget));
            const finance = data.financeAssets.find((x) => x.id === f.financeAssetId);
            void send({
              ...f,
              financeAssetId: finance ? String(finance.id) : undefined,
              financeAssetVersion: finance ? Number(finance.version) : undefined,
              departmentId: f.departmentId || null,
              warrantyEnd: f.warrantyEnd || null,
            },e.currentTarget);
          }}
        >
          <h2>Register asset</h2>
          <Field appearance="standard">
            Branch
            <Select appearance="standard" name="branchId" required>
              {data.nodes
                .filter((n) => ["branch", "headquarters"].includes(String(n.kind)))
                .map((n) => (
                  <option key={String(n.id)} value={String(n.id)}>
                    {String(n.display_name)}
                  </option>
                ))}
            </Select>
          </Field>
          <Field appearance="standard">
            Department
            <Select appearance="standard" name="departmentId">
              <option value="">No department</option>
              {data.nodes
                .filter((n) => n.kind === "department")
                .map((n) => (
                  <option key={String(n.id)} value={String(n.id)}>
                    {String(n.display_name)}
                  </option>
                ))}
            </Select>
          </Field>
          <Field appearance="standard">
            Asset code
            <TextInput appearance="standard" name="code" required />
          </Field>
          <Field appearance="standard">
            Name
            <TextInput appearance="standard" name="name" required />
          </Field>
          <Field appearance="standard">
            Category
            <TextInput appearance="standard" name="category" required />
          </Field>
          <Field appearance="standard">
            Currency
            <TextInput appearance="standard" name="currency" defaultValue="THB" maxLength={3} required />
          </Field>
          <Field appearance="standard">
            Acquisition cost
            <TextInput appearance="standard" name="cost" inputMode="decimal" defaultValue="0.00" required />
          </Field>
          {data.canLinkFinance && <Field appearance="standard">
            Recognized Finance asset
            <Select appearance="standard" name="financeAssetId">
              <option value="">Link after registration</option>
              {data.financeAssets.filter(x => x.status === "in_service").map(x => <option key={String(x.id)} value={String(x.id)}>{String(x.asset_number)} · {String(x.name)} · {String(x.currency)}</option>)}
            </Select>
          </Field>}
          <Field appearance="standard">
            Acquired date
            <TextInput appearance="standard" name="acquiredDate" type="date" required />
          </Field>
          <Field appearance="standard">
            Warranty end
            <TextInput appearance="standard" name="warrantyEnd" type="date" />
          </Field>
          <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy}>
            Register asset
          </Button>
        </FormLayout></AssetEditor>
      )}
      {r && action(
        "link-finance", "Link recognized Finance asset", <Field appearance="standard">
          Finance asset
          <Select appearance="standard" name="financeAssetId" required onChange={(e) => {
            const source = detail.financeAssets.find(x => x.id === e.currentTarget.value);
            const version = e.currentTarget.form?.elements.namedItem("financeAssetVersion") as HTMLInputElement | null;
            if (version) version.value = String(source?.version ?? "");
          }}>
            {detail.financeAssets.map(x => <option key={String(x.id)} value={String(x.id)}>{String(x.asset_number)} · {String(x.name)} · {String(x.currency)}</option>)}
          </Select>
          <input name="financeAssetVersion" type="hidden" defaultValue={String(detail.financeAssets[0]?.version ?? "")} />
        </Field>,Boolean(!r.finance_asset_id&&detail?.canLinkFinance),
      )}
      {r && action(
          "place",
          "Place into service",
          <>
            <Field appearance="standard">
              Placement
              <TextInput appearance="standard" name="placement" required />
            </Field>
            <Field appearance="standard">
              Custodian
              <Select appearance="standard" name="custodianId" required>
                {(data?.people ?? detail?.people ?? []).map((p) => (
                  <option key={String(p.id)} value={String(p.id)}>
                    {String(p.name)}
                  </option>
                ))}
              </Select>
            </Field>
          </>,["registered","in_service"].includes(String(r.lifecycle_status)),
        )}
      {r && action(
          "condition",
          "Record condition",
          <Field appearance="standard">
            Condition
            <Select appearance="standard" name="condition">
              <option value="good">Good</option>
              <option value="degraded">Degraded</option>
              <option value="unsafe">Unsafe</option>
            </Select>
          </Field>,r.lifecycle_status!=="retired",
        )}
      {r&&action("retire-request",r.lifecycle_status==="retirement_pending"?retirementReview.title:"Request retirement",r.lifecycle_status==="retirement_pending"?<p>{retirementReview.description}</p>:undefined,["registered","in_service"].includes(String(r.lifecycle_status))||r.lifecycle_status==="retirement_pending"&&retirementRequest?.actor_id===detail?.actorId)}
      {r&&action("retire-approve","Approve retirement",undefined,r.lifecycle_status==="retirement_pending"&&Number(r.can_review)>0)}
    </AssetRecoveryBoundary>
  );
}
export function MaintenanceConsole({
  base,
  data,
  detail,
}: {
  base: string;
  data?: Awaited<ReturnType<typeof maintenanceWorkspace>>;
  detail?: Awaited<ReturnType<typeof maintenanceDetail>>;
}) {
  const purchases=detail?.purchases??data?.purchases,{send,retry,busy,error,notice,saved,uncertain}=useAssetCommand(base,"maintenance",purchases,detail?.work),
    r = detail?.work;
  function action(name: string, title: string, extra?: React.ReactNode,available=true) {
    return (
      <AssetEditor key={name} title={title} busy={busy} error={error} visible={available} disabled={detail?.canCreate===false||!assetCommandAllowed("maintenance",{id:r?.id,action:name},purchases)}><FormLayout
        className="ui-panel ui-form-grid"
        onSubmit={(e) => {
          e.preventDefault();
          const f = Object.fromEntries(new FormData(e.currentTarget));
          void send({
            ...f,
            id: r!.id,
            action: name,
            expectedVersion: Number(r!.version),
            scheduledAt: f.scheduledAt ? new Date(String(f.scheduledAt)).getTime() : undefined,
          },e.currentTarget);
        }}
      >
        <h2>{title}</h2>
        {extra}
        <Field appearance="standard">
          Reason and evidence
          <TextArea appearance="standard" name="reason" required minLength={3} />
        </Field>
        <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy}>
          {title}
        </Button>
      </FormLayout></AssetEditor>
    );
  }
  return (
    <AssetRecoveryBoundary recovery={{retry,busy,uncertain,notice,saved}}>
      {error && <InlineNotice tone="danger" role="alert">{error}</InlineNotice>}{notice&&<InlineNotice role="status">{notice}</InlineNotice>}{saved&&<p>Saved original: <WorkspaceLink href={saved.href}>{saved.id}</WorkspaceLink></p>}{uncertain&&<Button appearance="standard" type="button" disabled={busy} onClick={()=>void retry()}>Retry submitted change</Button>}
      {data && (
        <AssetEditor title="Open work order" visible={data.canCreate} busy={busy} error={error} disabled={!assetCommandAllowed("maintenance",{},purchases)}><FormLayout
          className="ui-panel ui-form-grid"
          onSubmit={(e) => {
            e.preventDefault();
            const f = Object.fromEntries(new FormData(e.currentTarget)),
              asset = data.assets.find((x) => x.id === f.assetId);
            void send({
              ...f,
              assetVersion: Number(asset?.version),
              scheduledAt: f.scheduledAt ? new Date(String(f.scheduledAt)).getTime() : null,
            },e.currentTarget);
          }}
        >
          <h2>Open work order</h2>
          <Field appearance="standard">
            Asset
            <Select appearance="standard" name="assetId" required>
              {data.assets.map((x) => (
                <option key={String(x.id)} value={String(x.id)}>
                  {String(x.code)} · {String(x.name)}
                </option>
              ))}
            </Select>
          </Field>
          <Field appearance="standard">
            Title
            <TextInput appearance="standard" name="title" required />
          </Field>
          <Field appearance="standard">
            Priority
            <Select appearance="standard" name="priority">
              <option value="normal">Normal</option>
              <option value="high">High</option>
              <option value="critical">Critical</option>
            </Select>
          </Field>
          <Field appearance="standard">
            Owner
            <Select appearance="standard" name="ownerId" required>
              {data.people.map((p) => (
                <option key={String(p.id)} value={String(p.id)}>
                  {String(p.name)}
                </option>
              ))}
            </Select>
          </Field>
          <PreferredDateTimeField appearance="standard" name="scheduledAt" label="Schedule" />
          <Field appearance="standard">
            Reason
            <TextArea appearance="standard" name="reason" required minLength={3} />
          </Field>
          <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy}>
            Open work order
          </Button>
        </FormLayout></AssetEditor>
      )}
      {r && action(
          "schedule",
          "Schedule or reassign",
          <>
            <Field appearance="standard">
              Owner id
              <TextInput appearance="standard" name="ownerId" defaultValue={String(r.owner_id)} required />
            </Field>
            <PreferredDateTimeField appearance="standard" name="scheduledAt" label="Schedule" required />
          </>,["open","scheduled"].includes(String(r.status)),
        )}
      {r&&action("start","Start work",undefined,["open","scheduled"].includes(String(r.status)))}
      {r&&action("block","Block work",undefined,r.status==="in_progress")}
      {r&&detail&&<AssetEditor title="Issue spare parts" busy={busy} error={error} visible={r.status==="in_progress"} disabled={detail.canCreate===false||!assetCommandAllowed("maintenance",{id:r.id,action:"issue-part"},purchases)}><ServiceSpareForm spares={detail.spares} busy={busy} onSubmit={input => { void send({ ...input, id:r.id, action:"issue-part", expectedVersion:Number(r.version) },document.activeElement?.closest("form")??undefined); }} /></AssetEditor>}
      {r&&action("resume","Resume work",undefined,r.status==="blocked")}
      {r && action(
          "complete",
          "Complete work",
          <>
            <Field appearance="standard">
              Completion evidence
              <TextArea appearance="standard" name="evidence" required minLength={3} />
            </Field>
            <Field appearance="standard">
              Resulting condition
              <Select appearance="standard" name="resultingCondition">
                <option value="good">Good</option>
                <option value="degraded">Degraded</option>
                <option value="unsafe">Unsafe</option>
              </Select>
            </Field>
          </>,r.status==="in_progress",
        )}
      {r&&action("cancel","Cancel work",undefined,!["completed","cancelled"].includes(String(r.status)))}
    </AssetRecoveryBoundary>
  );
}
