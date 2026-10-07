"use client";
import { InlineNotice } from "@/components/ui/foundation";

import { FormDisclosure } from "@/components/ui/form-disclosure";
import { Button, Field, Select, TextArea, TextInput } from "@/components/ui/foundation";
import { PreferredDateTimeField } from "@/components/time/preferred-date-time-field";
import { useRef, useState } from "react";
import { useRouter } from "next/navigation";
import { workFetch } from "@/lib/work-fetch";
import { ServiceSpareForm } from "./service-spare-form";
import type {assetDetail,assetWorkspace} from "@/server/modules/assets/public/asset-maintenance";
import type {maintenanceDetail,maintenanceWorkspace} from "@/server/modules/maintenance/public/asset-maintenance";
import { FormLayout } from "@/components/ui/foundation";
function useCommand(base: string, path: string) {
  const router = useRouter(),
    last = useRef<{ key: string; body: string } | null>(null),
    [busy, setBusy] = useState(false),
    [error, setError] = useState("");
  async function send(input: unknown) {
    if (busy) return;
    setBusy(true);
    setError("");
    const body = JSON.stringify(input);
    if (last.current?.body !== body) last.current = { key: crypto.randomUUID(), body };
    try {
      const response = await workFetch(`/api/v1${base}/${path}`, {
          method: "POST",
          headers: { "Content-Type": "application/json", "Idempotency-Key": last.current.key },
          body,
        }),
        result = await response.json();
      if (!response.ok) {
        if (response.status < 500) last.current = null;
        throw new Error(result.error?.message ?? "Unable to record this action.");
      }
      last.current = null;
      router.push(`${base}/${path}/${result.data.id}`);
      router.refresh();
    } catch (e) {
      setError(e instanceof Error ? e.message : "Retry this action.");
    } finally {
      setBusy(false);
    }
  }
  return { send, busy, error };
}
export function AssetConsole({
  base,
  data,
  detail,
}: {
  base: string;
  data?: Awaited<ReturnType<typeof assetWorkspace>>;
  detail?: Awaited<ReturnType<typeof assetDetail>>;
}) {
  const { send, busy, error } = useCommand(base, "assets"),
    r = detail?.asset;
  function action(name: string, title: string, extra?: React.ReactNode) {
    return (
      <FormLayout
        className="ui-panel ui-form-grid"
        onSubmit={(e) => {
          e.preventDefault();
          void send({
            ...Object.fromEntries(new FormData(e.currentTarget)),
            ...(name === "link-finance" ? {financeAssetVersion: Number(new FormData(e.currentTarget).get("financeAssetVersion"))} : {}),
            id: r!.id,
            action: name,
            expectedVersion: Number(r!.version),
          });
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
      </FormLayout>
    );
  }
  return (
    <>
      {error && <InlineNotice tone="danger" role="alert">{error}</InlineNotice>}
      {data?.canCreate && (
        <FormDisclosure appearance="standard" className="ui-form-disclosure" dismissDisabled={busy}><summary>Register asset</summary><FormLayout
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
            });
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
        </FormLayout></FormDisclosure>
      )}
      {r && !r.finance_asset_id && detail?.canLinkFinance && action(
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
        </Field>,
      )}
      {r &&
        ["registered", "in_service"].includes(String(r.lifecycle_status)) &&
        action(
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
          </>,
        )}
      {r &&
        r.lifecycle_status !== "retired" &&
        action(
          "condition",
          "Record condition",
          <Field appearance="standard">
            Condition
            <Select appearance="standard" name="condition">
              <option value="good">Good</option>
              <option value="degraded">Degraded</option>
              <option value="unsafe">Unsafe</option>
            </Select>
          </Field>,
        )}
      {r &&
        ["registered", "in_service"].includes(String(r.lifecycle_status)) &&
        action("retire-request", "Request retirement")}
      {r &&
        r.lifecycle_status === "retirement_pending" &&
        Number(r.can_review) > 0 &&
        action("retire-approve", "Approve retirement")}
    </>
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
  const { send, busy, error } = useCommand(base, "maintenance"),
    r = detail?.work;
  function action(name: string, title: string, extra?: React.ReactNode) {
    return (
      <FormLayout
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
          });
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
      </FormLayout>
    );
  }
  return (
    <>
      {error && <InlineNotice tone="danger" role="alert">{error}</InlineNotice>}
      {data?.canCreate && (
        <FormDisclosure appearance="standard" className="ui-form-disclosure" dismissDisabled={busy}><summary>Open work order</summary><FormLayout
          className="ui-panel ui-form-grid"
          onSubmit={(e) => {
            e.preventDefault();
            const f = Object.fromEntries(new FormData(e.currentTarget)),
              asset = data.assets.find((x) => x.id === f.assetId);
            void send({
              ...f,
              assetVersion: Number(asset?.version),
              scheduledAt: f.scheduledAt ? new Date(String(f.scheduledAt)).getTime() : null,
            });
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
        </FormLayout></FormDisclosure>
      )}
      {r &&
        ["open", "scheduled"].includes(String(r.status)) &&
        action(
          "schedule",
          "Schedule or reassign",
          <>
            <Field appearance="standard">
              Owner id
              <TextInput appearance="standard" name="ownerId" defaultValue={String(r.owner_id)} required />
            </Field>
            <PreferredDateTimeField appearance="standard" name="scheduledAt" label="Schedule" required />
          </>,
        )}
      {r && ["open", "scheduled"].includes(String(r.status)) && action("start", "Start work")}
      {r && r.status === "in_progress" && action("block", "Block work")}
      {r && detail && r.status === "in_progress" && <ServiceSpareForm spares={detail.spares} busy={busy} onSubmit={input => { void send({ ...input, id:r.id, action:"issue-part", expectedVersion:Number(r.version) }); }} />}
      {r && r.status === "blocked" && action("resume", "Resume work")}
      {r &&
        r.status === "in_progress" &&
        action(
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
          </>,
        )}
      {r &&
        !["completed", "cancelled"].includes(String(r.status)) &&
        action("cancel", "Cancel work")}
    </>
  );
}
