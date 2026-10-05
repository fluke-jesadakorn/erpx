"use client";
import { InlineNotice } from "@/components/ui/foundation";

import { FormDisclosure } from "@/components/ui/form-disclosure";
import { Button, Field, Select, TextArea, TextInput } from "@/components/ui/foundation";

import { useRef, useState } from "react";
import { useRouter } from "next/navigation";
import { workFetch } from "@/lib/work-fetch";
import type { benefitDetail, benefitsWorkspace } from "@/server/benefits";
import {
  AttachmentPanel,
  type AttachmentPanelChange,
} from "@/components/attachments/attachment-panel";
import { RecordDocumentPanel } from "@/components/attachments/record-document-panel";
import { FormLayout } from "@/components/ui/foundation";
function useCommand(base: string) {
  const router = useRouter(),
    last = useRef<{ key: string; body: string } | null>(null),
    [busy, setBusy] = useState(false),
    [error, setError] = useState("");
  async function send(input: unknown) {
    const body = JSON.stringify(input);
    if (last.current?.body !== body) last.current = { key: crypto.randomUUID(), body };
    setBusy(true);
    setError("");
    try {
      const r = await workFetch(`/api/v1${base}/benefits`, {
          method: "POST",
          headers: {
            "Content-Type": "application/json",
            "Idempotency-Key": last.current.key,
          },
          body,
        }),
        j = await r.json();
      if (!r.ok) {
        if (r.status < 500) last.current = null;
        throw new Error(j.error?.message ?? "Unable to update benefits.");
      }
      last.current = null;
      router.push(
        `${base}/benefits${input && typeof input === "object" && "entity" in input && input.entity === "election" ? `/${j.data.id}` : ""}`,
      );
      router.refresh();
    } catch (e) {
      setError(e instanceof Error ? e.message : "Retry this action.");
    } finally {
      setBusy(false);
    }
  }
  return { send, busy, error };
}
export function BenefitsConsole({ recordOnly=false,
  base,
  data,
  detail,
}: { recordOnly?:boolean;
  base: string;
  data?: Awaited<ReturnType<typeof benefitsWorkspace>>;
  detail?: Awaited<ReturnType<typeof benefitDetail>>;
}) {
  const { send, busy, error } = useCommand(base),
    e = detail?.election,
    [transitionFiles, setTransitionFiles] = useState<AttachmentPanelChange | null>(null),
    [dependentFiles, setDependentFiles] = useState<Record<string, AttachmentPanelChange>>({});
  return (
    <>
      {error && <InlineNotice tone="danger" role="alert">{error}</InlineNotice>}
      {data?.canReview && (
        <>
          {!recordOnly && <FormDisclosure appearance="standard" className="ui-form-disclosure" dismissDisabled={busy}><summary>Draft benefit plan</summary><FormLayout
            className="ui-panel ui-form-grid"
            onSubmit={(x) => {
              x.preventDefault();
              const f = Object.fromEntries(new FormData(x.currentTarget));
              void send({
                ...f,
                entity: "plan",
                effectiveTo: f.effectiveTo || null,
                requiredEligibilityCode:f.requiredEligibilityCode||null,
              });
            }}
          >
            <h2>Draft benefit plan</h2>
            <Field appearance="standard">
              Name
              <TextInput appearance="standard" name="name" required />
            </Field>
            <Field appearance="standard">
              Provider
              <TextInput appearance="standard" name="provider" required />
            </Field>
            <Field appearance="standard">
              Coverage
              <Select appearance="standard" name="coverageType">
                <option value="employee">Employee</option>
                <option value="family">Family</option>
              </Select>
            </Field>
            <Field appearance="standard">
              Currency
              <TextInput appearance="standard" name="currency" defaultValue="THB" required />
            </Field>
            <Field appearance="standard">
              Employee deduction
              <TextInput appearance="standard" name="employeeDeduction" inputMode="decimal" required />
            </Field>
            <Field appearance="standard">
              Employer cost
              <TextInput appearance="standard" name="employerCost" inputMode="decimal" required />
            </Field>
            <Field appearance="standard">
              Effective from
              <TextInput appearance="standard" name="effectiveFrom" type="date" required />
            </Field>
            <Field appearance="standard">
              Effective to
              <TextInput appearance="standard" name="effectiveTo" type="date" />
            </Field>
            <Field appearance="standard">Required learning eligibility code, when applicable<TextInput appearance="standard" name="requiredEligibilityCode" maxLength={120}/></Field>
            <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy}>
              Draft plan
            </Button>
          </FormLayout></FormDisclosure>}
          {!recordOnly && <FormLayout
            className="ui-panel ui-form-grid"
            onSubmit={(x) => {
              x.preventDefault();
              void send({
                ...Object.fromEntries(new FormData(x.currentTarget)),
                entity: "window",
              });
            }}
          >
            <h2>Draft enrollment window</h2>
            <Field appearance="standard">
              Name
              <TextInput appearance="standard" name="name" required />
            </Field>
            <Field appearance="standard">
              Opens
              <TextInput appearance="standard" name="opensOn" type="date" required />
            </Field>
            <Field appearance="standard">
              Closes
              <TextInput appearance="standard" name="closesOn" type="date" required />
            </Field>
            <Field appearance="standard">
              Coverage effective
              <TextInput appearance="standard" name="effectiveOn" type="date" required />
            </Field>
            <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy}>
              Draft window
            </Button>
          </FormLayout>}
          {(
            [
              ...data.plans
                .filter((x) => x.status === "draft")
                .map((x) => ({ entity: "plan", ...x })),
              ...data.windows
                .filter((x) => x.status === "draft")
                .map((x) => ({ entity: "window", ...x })),
            ] as Array<Record<string, unknown>>
          ).map((x) => (
            <FormLayout
              className="ui-panel ui-form-grid"
              key={String(x.id)}
              onSubmit={(ev) => {
                ev.preventDefault();
                void send({
                  entity: x.entity,
                  id: x.id,
                  action: x.entity === "plan" ? "publish" : "open",
                  expectedVersion: Number(x.version),
                  reason: String(new FormData(ev.currentTarget).get("reason")),
                });
              }}
            >
              <h2>Independent review · {String(x.name)}</h2>
              <Field appearance="standard">
                Reason
                <TextArea appearance="standard" name="reason" minLength={3} required />
              </Field>
              <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy}>
                {x.entity === "plan" ? "Publish plan" : "Open window"}
              </Button>
            </FormLayout>
          ))}
        </>
      )}
      {data?.canWrite && (
        (!recordOnly && <FormLayout
          className="ui-panel ui-form-grid"
          onSubmit={(x) => {
            x.preventDefault();
            void send({
              ...Object.fromEntries(new FormData(x.currentTarget)),
              entity: "dependent",
            });
          }}
        >
          <h2>Add dependent</h2>
          <Field appearance="standard">
            Name
            <TextInput appearance="standard" name="name" required />
          </Field>
          <Field appearance="standard">
            Relationship
            <TextInput appearance="standard" name="relationship" required />
          </Field>
          <Field appearance="standard">
            Birth date
            <TextInput appearance="standard" name="birthDate" type="date" required />
          </Field>
          <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy}>
            Submit dependent
          </Button>
        </FormLayout>)
      )}
      {data?.plans
        .filter((plan) => ["draft", "published", "retired"].includes(String(plan.status)))
        .map((plan) => (
          <RecordDocumentPanel appearance="standard"
            key={`booklet:${String(plan.id)}`}
            base={base}
            targetType="benefit_plan"
            targetId={String(plan.id)}
            expectedTargetVersion={Number(plan.version)}
            editable={data.canManageDocuments}
            title={`${String(plan.name)} · plan documents`}
            purposes={[
              {
                key: "benefit_booklet",
                label: "Benefit booklet",
                description:
                  "Attach the reviewed plan booklet or coverage guide for this plan revision.",
                visibility:
                  "Published booklets are self-visible to active employees; drafts remain restricted to Benefits document managers.",
              },
            ]}
          />
        ))}
      {data?.dependents.map((dependent) => (
        <RecordDocumentPanel appearance="standard"
          key={`dependent-proof:${String(dependent.id)}`}
          base={base}
          targetType="benefit_dependent"
          targetId={String(dependent.id)}
          expectedTargetVersion={Number(dependent.version)}
          editable={String(dependent.status) === "pending"}
          title={`${String(dependent.name)} · dependent proof`}
          purposes={[
            {
              key: "dependent_proof",
              label: "Dependent proof",
              description: "Attach relationship, identity, or provider eligibility evidence.",
              visibility:
                "Only the employee owner and dedicated Benefits document roles can access these files.",
            },
          ]}
        />
      ))}
      {data?.canReview &&
        data.dependents
          .filter((x) => x.status === "pending")
          .map((x) => (
            <FormLayout
              className="ui-panel ui-form-grid"
              key={String(x.id)}
              onSubmit={(ev) => {
                ev.preventDefault();
                void send({
                  entity: "dependent",
                  id: x.id,
                  action: "verify",
                  expectedVersion: Number(x.version),
                  reason: String(new FormData(ev.currentTarget).get("reason")),
                  attachmentSetIds:
                    dependentFiles[String(x.id)]?.setId &&
                    dependentFiles[String(x.id)]?.readyCount > 0 &&
                    dependentFiles[String(x.id)]?.bindable
                      ? [dependentFiles[String(x.id)].setId]
                      : [],
                });
              }}
            >
              <h2>Verify dependent · {String(x.name)}</h2>
              <Field appearance="standard">
                Reason
                <TextArea appearance="standard" name="reason" required minLength={3} />
              </Field>
              {data.canManageDocuments && (
                <AttachmentPanel appearance="standard"
                  base={base}
                  targetType="benefit_dependent"
                  targetId={String(x.id)}
                  expectedTargetVersion={Number(x.version)}
                  componentKind="action"
                  componentRef="verification"
                  purposeKey="dependent_proof"
                  title="Dependent verification proof"
                  description="Attach the relationship, identity, or provider evidence reviewed for this verification."
                  visibility="Restricted to the employee owner and dedicated Benefits document roles."
                  onChange={(change) =>
                    setDependentFiles((current) => ({
                      ...current,
                      [String(x.id)]: change,
                    }))
                  }
                />
              )}
              <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy}>
                Verify
              </Button>
            </FormLayout>
          ))}
      {data?.canWrite && (
        (!recordOnly && <FormLayout
          className="ui-panel ui-form-grid"
          onSubmit={(x) => {
            x.preventDefault();
            const f = Object.fromEntries(new FormData(x.currentTarget)),
              plan = data.plans.find((p) => p.id === f.planId),
              win = data.windows.find((w) => w.id === f.windowId);
            void send({
              entity: "election",
              planId: f.planId,
              planVersion: Number(plan?.version),
              windowId: f.windowId,
              windowVersion: Number(win?.version),
              dependentIds: new FormData(x.currentTarget).getAll("dependentIds"),
            });
          }}
        >
          <h2>Elect coverage</h2>
          <Field appearance="standard">
            Plan
            <Select appearance="standard" name="planId">
              {data.plans
                .filter((x) => x.status === "published")
                .map((x) => (
                  <option key={String(x.id)} value={String(x.id)}>
                    {String(x.name)}
                  </option>
                ))}
            </Select>
          </Field>
          <Field appearance="standard">
            Window
            <Select appearance="standard" name="windowId">
              {data.windows
                .filter((x) => x.status === "open")
                .map((x) => (
                  <option key={String(x.id)} value={String(x.id)}>
                    {String(x.name)}
                  </option>
                ))}
            </Select>
          </Field>
          <fieldset>
            <legend>Verified dependents</legend>
            {data.dependents
              .filter((x) => x.status === "verified")
              .map((x) => (
                <Field appearance="standard" orientation="inline" key={String(x.id)}>
                  <TextInput appearance="standard" type="checkbox" name="dependentIds" value={String(x.id)} />
                  {String(x.name)}
                </Field>
              ))}
          </fieldset>
          <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy}>
            Elect plan
          </Button>
        </FormLayout>)
      )}
      {e && !["released", "cancelled"].includes(String(e.status)) && (
        (!recordOnly && <FormLayout
          className="ui-panel ui-form-grid"
          onSubmit={(x) => {
            x.preventDefault();
            const f = Object.fromEntries(new FormData(x.currentTarget)),
              action =
                e.status === "elected"
                  ? "submit"
                  : e.status === "submitted"
                    ? "acknowledge"
                    : "release";
            void send({
              entity: "election",
              id: e.id,
              action,
              expectedVersion: Number(e.version),
              reason: f.reason,
              providerReference: f.providerReference || undefined,
              attachmentSetIds:
                transitionFiles?.setId && transitionFiles.readyCount > 0 && transitionFiles.bindable
                  ? [transitionFiles.setId]
                  : [],
            });
          }}
        >
          <h2>
            {e.status === "elected"
              ? "Submit election"
              : e.status === "submitted"
                ? "Record provider acknowledgement"
                : "Release payroll input"}
          </h2>
          {e.status === "submitted" && (
            <Field appearance="standard">
              Provider reference
              <TextInput appearance="standard" name="providerReference" required />
            </Field>
          )}
          <Field appearance="standard">
            Reason
            <TextArea appearance="standard" name="reason" required minLength={3} />
          </Field>
          {e.status === "elected" && (
            <AttachmentPanel appearance="standard"
              key={`${String(e.id)}:election:${String(e.version)}`}
              base={base}
              targetType="benefit_election"
              targetId={String(e.id)}
              expectedTargetVersion={Number(e.version)}
              componentKind="action"
              componentRef="employee-submit"
              purposeKey="election_document"
              title="Election documents"
              description="Attach employee election or coverage-selection evidence. Clean files are locked with submission."
              visibility="Only the employee owner and dedicated Benefits document roles can access these files."
              onChange={setTransitionFiles}
            />
          )}
          {e.status === "submitted" && detail?.canManageDocuments && (
            <AttachmentPanel appearance="standard"
              key={`${String(e.id)}:provider:${String(e.version)}`}
              base={base}
              targetType="benefit_election"
              targetId={String(e.id)}
              expectedTargetVersion={Number(e.version)}
              componentKind="action"
              componentRef="provider-acknowledgement"
              purposeKey="provider_proof"
              title="Provider acknowledgement proof"
              description="Attach the provider receipt, acknowledgement, or enrollment confirmation."
              visibility="Restricted to dedicated Benefits document roles until acknowledgement is recorded."
              onChange={setTransitionFiles}
            />
          )}
          <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy}>
            Continue
          </Button>
        </FormLayout>)
      )}
    </>
  );
}
