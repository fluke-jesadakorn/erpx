"use client";
import { InlineNotice } from "@/components/ui/foundation";

import { FormDisclosure } from "@/components/ui/form-disclosure";
import { Button, Field, Select, TextArea, TextInput } from "@/components/ui/foundation";

import { useRef, useState } from "react";
import { useRouter } from "next/navigation";
import { workFetch } from "@/lib/work-fetch";
import { marketingAssetDraft, marketingAssetEditorSchema } from "@/lib/marketing-editor";
import type { marketingDetail, marketingWorkspace } from "@/server/modules/marketing/public/marketing";
import { FormLayout, Card } from "@/components/ui/foundation";
function displayMinor(minor: string, currency: string) {
  const amount = BigInt(minor),
    scale = BigInt(100),
    whole = amount / scale,
    fraction = amount % scale;
  return `${currency} ${new Intl.NumberFormat("en-GB").format(whole)}.${fraction.toString().padStart(2, "0")}`;
}
export function MarketingConsole({
  base,
  data,
  detail,
}: {
  base: string;
  data?: Awaited<ReturnType<typeof marketingWorkspace>>;
  detail?: Awaited<ReturnType<typeof marketingDetail>>;
}) {
  const router = useRouter(),
    last = useRef<{ body: string; key: string } | null>(null),
    [busy, setBusy] = useState(false),
    [error, setError] = useState(""),
    [assetKind, setAssetKind] = useState<"form" | "content" | "domain" | "provider">("form"),
    [assetName, setAssetName] = useState(""),
    [headline, setHeadline] = useState(""),
    [summary, setSummary] = useState(""),
    [reference, setReference] = useState(""),
    [contentAssetId,setContentAssetId]=useState(""),
    [mediaPublicationId,setMediaPublicationId]=useState(""),
    [consentRequired, setConsentRequired] = useState(false);
  async function send(input: unknown) {
    const body = JSON.stringify(input);
    if (last.current?.body !== body) last.current = { body, key: crypto.randomUUID() };
    setBusy(true);
    setError("");
    try {
      const r = await workFetch(`/api/v1${base}/marketing`, {
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
        throw new Error(j.error?.message ?? "Unable to update Marketing.");
      }
      last.current = null;
      if (
        input &&
        typeof input === "object" &&
        "entity" in input &&
        input.entity === "candidate" &&
        !("id" in input)
      )
        router.push(`${base}/marketing/${j.data.id}`);
      router.refresh();
    } catch (e) {
      setError(e instanceof Error ? e.message : "Retry this action.");
    } finally {
      setBusy(false);
    }
  }
  const vals = (e: React.FormEvent<HTMLFormElement>) =>
      Object.fromEntries(new FormData(e.currentTarget)),
    c = detail?.candidate;
  return (
    <>
      {error && <InlineNotice tone="danger" role="alert">{error}</InlineNotice>}
      {data && (
        <>
          <FormLayout
            className="ui-panel ui-form-grid"
            onSubmit={(e) => {
              e.preventDefault();
              const f = vals(e);
              void send({ entity: "campaign", action: "create", code: f.code, name: f.name });
            }}
          >
            <h2>Create campaign ID</h2>
            <Field appearance="standard">
              Campaign code
              <TextInput appearance="standard" name="code" required pattern="[A-Za-z0-9_-]+" />
            </Field>
            <Field appearance="standard">
              Name
              <TextInput appearance="standard" name="name" required />
            </Field>
            <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy}>
              Create campaign
            </Button>
          </FormLayout>
          {data.canLinkCost && (
            <FormLayout
              className="ui-panel ui-form-grid"
              onSubmit={(e) => {
                e.preventDefault();
                const f = vals(e);
                void send({
                  entity: "campaign",
                  action: "link-cost",
                  campaignId: f.campaignId,
                  journalLineId: f.journalLineId,
                  evidence: f.evidence,
                });
              }}
            >
              <h2>Link posted campaign cost</h2>
              <p>Choose a posted expense line. Each line can belong to one campaign only.</p>
              <Field appearance="standard">
                Campaign
                <Select appearance="standard" name="campaignId" required>
                  <option value="">Select campaign</option>
                  {data.campaigns.map((c) => (
                    <option key={String(c.id)} value={String(c.id)}>
                      {String(c.code)} · {String(c.name)}
                    </option>
                  ))}
                </Select>
              </Field>
              <Field appearance="standard">
                Posted expense
                <Select appearance="standard" name="journalLineId" required disabled={!data.costSources.length}>
                  <option value="">Select an expense line</option>
                  {data.costSources.map((line) => (
                    <option key={line.source_line_id} value={line.source_line_id}>
                      {line.entry_date} · {line.journal_number} · {line.account_code}{" "}
                      {line.account_name} · {displayMinor(line.debit_minor, line.currency)}
                    </option>
                  ))}
                </Select>
              </Field>
              {!data.costSources.length && <p>No unlinked posted expense lines are available.</p>}
              <Field appearance="standard">
                Why does this cost belong to the campaign?
                <TextArea appearance="standard" name="evidence" minLength={3} maxLength={2000} required />
              </Field>
              <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy || !data.costSources.length}>
                Link cost
              </Button>
            </FormLayout>
          )}
          <FormDisclosure appearance="standard" className="ui-form-disclosure" dismissDisabled={busy}><summary>Draft marketing asset</summary><FormLayout
            className="ui-panel ui-form-grid"
            onSubmit={(e) => {
              e.preventDefault();
              const result = marketingAssetEditorSchema.safeParse({
                kind: assetKind,
                name: assetName,
                headline,
                summary,
                reference,
                consentRequired,
                ...(contentAssetId?{contentAssetId,contentVersion:Number(data.assets.find(asset=>asset.id===contentAssetId)?.version)}:{}),
                ...(mediaPublicationId?{mediaPublicationId}:{}),
              });
              if (!result.success) {
                setError(result.error.issues[0]?.message ?? "Review the asset details.");
                return;
              }
              void send({ entity: "asset", ...marketingAssetDraft(result.data) });
            }}
          >
            <h2>Draft marketing asset</h2>
            <Field appearance="standard">
              Kind
              <Select appearance="standard"
                name="kind"
                value={assetKind}
                onChange={(e) => setAssetKind(e.target.value as typeof assetKind)}
              >
                <option value="form">Form</option>
                <option value="content">Content</option>
                <option value="domain">Domain</option>
                <option value="provider">Provider</option>
              </Select>
            </Field>
            <Field appearance="standard">
              Name
              <TextInput appearance="standard"
                name="name"
                value={assetName}
                onChange={(e) => setAssetName(e.target.value)}
                minLength={2}
                maxLength={160}
                required
              />
            </Field>
            {assetKind === "form" ? (
              <>
                <p>
                  This form will ask visitors for their name, contact details and request. A
                  separate reviewer must activate it.
                </p>
                <Field appearance="standard">
                  Public headline
                  <TextInput appearance="standard"
                    value={headline}
                    onChange={(e) => setHeadline(e.target.value)}
                    minLength={2}
                    maxLength={160}
                    required
                  />
                </Field>
                <Field appearance="standard">
                  Supporting text
                  <TextArea appearance="standard"
                    value={summary}
                    onChange={(e) => setSummary(e.target.value)}
                    maxLength={500}
                  />
                </Field>
                <Field appearance="standard" orientation="inline">
                  <TextInput appearance="standard"
                    type="checkbox"
                    checked={consentRequired}
                    onChange={(e) => setConsentRequired(e.target.checked)}
                    required
                  />{" "}
                  Require visitor consent before submitting
                </Field>
                <Field appearance="standard">
                  Approved supporting content
                  <Select appearance="standard" name="contentAssetId" value={contentAssetId} onChange={e=>{setContentAssetId(e.target.value);setMediaPublicationId('');}}>
                    <option value="">Form headline only</option>
                    {data.assets.filter(asset=>asset.kind==='content'&&asset.status==='active').map(asset=><option key={String(asset.id)} value={String(asset.id)}>{String(asset.name)} · revision {Number(asset.version)}</option>)}
                  </Select>
                </Field>
                {contentAssetId&&<Field appearance="standard">
                  Approved public creative
                  <Select appearance="standard" name="mediaPublicationId" value={mediaPublicationId} onChange={e=>setMediaPublicationId(e.target.value)}>
                    <option value="">Supporting text only</option>
                    {data.contentPublications.find(source=>source.assetId===contentAssetId)?.items.map(publication=><option key={String(publication.id)} value={String(publication.id)}>{String(publication.alt_text)} · revision {Number(publication.revision)}</option>)}
                  </Select>
                </Field>}
              </>
            ) : (
              <>
                <Field appearance="standard">
                  Evidence or reference
                  <TextInput appearance="standard"
                    value={reference}
                    onChange={(e) => setReference(e.target.value)}
                    minLength={2}
                    maxLength={160}
                    required
                    placeholder="Document, URL or review reference"
                  />
                </Field>
                <Field appearance="standard">
                  What should reviewers know?
                  <TextArea appearance="standard"
                    value={summary}
                    onChange={(e) => setSummary(e.target.value)}
                    minLength={3}
                    maxLength={500}
                    required
                  />
                </Field>
              </>
            )}
            <aside className="ui-panel" aria-label="Asset preview">
              <strong>Preview for review</strong>
              <h3>{assetName.trim() || "Asset name"}</h3>
              {assetKind === "form" ? (
                <>
                  <p>{headline.trim() || "Public headline"}</p>
                  {summary.trim() && <p>{summary.trim()}</p>}
                  <small>
                    {consentRequired ? "Consent required" : "Choose consent before saving"}
                  </small>
                </>
              ) : (
                <>
                  <p>{summary.trim() || "Review summary"}</p>
                  <small>{reference.trim() || "Evidence reference"}</small>
                </>
              )}
            </aside>
            <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy}>
              Draft asset
            </Button>
          </FormLayout></FormDisclosure>
          <Card as="section" appearance="standard" className="ui-panel">
            <h2>Public form readiness</h2>
            {data.formReadiness.length?data.formReadiness.map(readiness=>{const asset=data.assets.find(asset=>asset.id===readiness.id);return <article key={readiness.id}><h3>{String(asset?.name)}</h3><p>{asset?.status==='active'&&readiness.ready?'Ready for consented public submissions':asset?.status==='active'?'Unavailable until supporting content and creative are approved':'Awaiting independent form activation'}</p>{asset?.status==='active'&&readiness.ready&&<a href={`/forms/${encodeURIComponent(base.split('/')[2])}/${encodeURIComponent(readiness.id)}`} target="_blank" rel="noreferrer">Open public form</a>}</article>}):<p>No forms have been drafted.</p>}
          </Card>
          {data.assets
            .filter((x) => x.status === "draft")
            .map((x) => (
              <FormLayout
                className="ui-panel ui-form-grid"
                key={String(x.id)}
                onSubmit={(e) => {
                  e.preventDefault();
                  void send({
                    entity: "asset",
                    id: x.id,
                    action: "activate",
                    expectedVersion: Number(x.version),
                    reason: String(new FormData(e.currentTarget).get("reason")),
                  });
                }}
              >
                <h2>Independent review · {String(x.name)}</h2>
                <Field appearance="standard">
                  Reason
                  <TextArea appearance="standard" name="reason" required minLength={3} />
                </Field>
                <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy}>
                  Activate
                </Button>
              </FormLayout>
            ))}
          <FormDisclosure appearance="standard" className="ui-form-disclosure" dismissDisabled={busy}><summary>Capture consented lead</summary><FormLayout
            className="ui-panel ui-form-grid"
            onSubmit={(e) => {
              e.preventDefault();
              const f = vals(e),
                form = data.assets.find((x) => x.id === f.formId);
              void send({
                ...f,
                entity: "candidate",
                formVersion: Number(form?.version),
                attribution: {
                  campaign: String(f.campaign),
                  source: String(f.source),
                  campaignId: String(f.campaignId ?? ""),
                },
              });
            }}
          >
            <h2>Capture consented lead</h2>
            <Field appearance="standard">
              Active form
              <Select appearance="standard" name="formId">
                {data.assets
                  .filter((x) => x.kind === "form" && x.status === "active")
                  .map((x) => (
                    <option key={String(x.id)} value={String(x.id)}>
                      {String(x.name)}
                    </option>
                  ))}
              </Select>
            </Field>
            <Field appearance="standard">
              Branch
              <Select appearance="standard" name="branchId">
                {data.branches.map((x) => (
                  <option key={String(x.id)} value={String(x.id)}>
                    {String(x.name)}
                  </option>
                ))}
              </Select>
            </Field>
            <Field appearance="standard">
              Contact name
              <TextInput appearance="standard" name="contactName" required />
            </Field>
            <Field appearance="standard">
              Contact detail
              <TextArea appearance="standard" name="contactDetail" required minLength={3} />
            </Field>
            <Field appearance="standard">
              Consent evidence
              <TextArea appearance="standard" name="consentEvidence" required minLength={3} />
            </Field>
            <Field appearance="standard">
              Campaign
              <TextInput appearance="standard" name="campaign" required />
            </Field>
            <Field appearance="standard">
              Reviewed campaign ID
              <Select appearance="standard" name="campaignId">
                <option value="">No linked campaign</option>
                {data.campaigns.map((x) => (
                  <option key={String(x.id)} value={String(x.id)}>
                    {String(x.code)} · {String(x.name)}
                  </option>
                ))}
              </Select>
            </Field>
            <Field appearance="standard">
              Source
              <TextInput appearance="standard" name="source" required />
            </Field>
            <Field appearance="standard">
              Brief
              <TextArea appearance="standard" name="brief" required minLength={3} />
            </Field>
            <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy}>
              Create lead candidate
            </Button>
          </FormLayout></FormDisclosure>
        </>
      )}
      {c && c.status === "pending" && (
        <>
          <FormLayout
            className="ui-panel ui-form-grid"
            onSubmit={(e) => {
              e.preventDefault();
              const f = vals(e);
              void send({
                entity: "candidate",
                id: c.id,
                action: "record-engagement",
                expectedVersion: Number(c.version),
                eventType: f.eventType,
                channel: f.channel,
                reason: f.reason,
              });
            }}
          >
            <h2>Record engagement</h2>
            <Field appearance="standard">
              Event
              <TextInput appearance="standard" name="eventType" required />
            </Field>
            <Field appearance="standard">
              Channel
              <TextInput appearance="standard" name="channel" required />
            </Field>
            <Field appearance="standard">
              Evidence
              <TextArea appearance="standard" name="reason" required minLength={3} />
            </Field>
            <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy}>
              Record
            </Button>
          </FormLayout>
          <FormLayout
            className="ui-panel ui-form-grid"
            onSubmit={(e) => {
              e.preventDefault();
              const f = vals(e);
              void send({
                entity: "candidate",
                id: c.id,
                action: "handoff",
                expectedVersion: Number(c.version),
                reason: f.reason,
                dueDate: f.dueDate,
              });
            }}
          >
            <h2>Independent CRM handoff</h2>
            <Field appearance="standard">
              Due date
              <TextInput appearance="standard" name="dueDate" type="date" required />
            </Field>
            <Field appearance="standard">
              Review evidence
              <TextArea appearance="standard" name="reason" required minLength={3} />
            </Field>
            <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy}>
              Create CRM lead
            </Button>
          </FormLayout>
        </>
      )}
    </>
  );
}
