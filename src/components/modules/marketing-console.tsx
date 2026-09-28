"use client";
import { useRef, useState } from "react";
import { useRouter } from "next/navigation";
import { workFetch } from "@/lib/work-fetch";
import { marketingAssetDraft, marketingAssetEditorSchema } from "@/lib/marketing-editor";
import type { marketingDetail, marketingWorkspace } from "@/server/marketing";
function displayMinor(minor: string, currency: string) {
  const amount = BigInt(minor), scale = BigInt(100), whole = amount / scale, fraction = amount % scale;
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
    [consentRequired, setConsentRequired] = useState(false);
  async function send(input: unknown) {
    const body = JSON.stringify(input);
    if (last.current?.body !== body)
      last.current = { body, key: crypto.randomUUID() };
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
      {error && <p role="alert">{error}</p>}
      {data && (
        <>
          <form className="card business-form" onSubmit={(e)=>{e.preventDefault();const f=vals(e);void send({entity:'campaign',action:'create',code:f.code,name:f.name});}}>
            <h2>Create campaign ID</h2>
            <label>Campaign code<input name="code" required pattern="[A-Za-z0-9_-]+" /></label>
            <label>Name<input name="name" required /></label>
            <button className="button" disabled={busy}>Create campaign</button>
          </form>
          {data.canLinkCost&&<form className="card business-form" onSubmit={(e)=>{e.preventDefault();const f=vals(e);void send({entity:'campaign',action:'link-cost',campaignId:f.campaignId,journalLineId:f.journalLineId,evidence:f.evidence});}}>
            <h2>Link posted campaign cost</h2>
            <p>Choose a posted expense line. Each line can belong to one campaign only.</p>
            <label>Campaign<select name="campaignId" required><option value="">Select campaign</option>{data.campaigns.map(c=><option key={String(c.id)} value={String(c.id)}>{String(c.code)} · {String(c.name)}</option>)}</select></label>
            <label>Posted expense<select name="journalLineId" required disabled={!data.costSources.length}>
              <option value="">Select an expense line</option>
              {data.costSources.map(line=><option key={line.source_line_id} value={line.source_line_id}>
                {line.entry_date} · {line.journal_number} · {line.account_code} {line.account_name} · {displayMinor(line.debit_minor,line.currency)}
              </option>)}
            </select></label>
            {!data.costSources.length&&<p>No unlinked posted expense lines are available.</p>}
            <label>Why does this cost belong to the campaign?<textarea name="evidence" minLength={3} maxLength={2000} required /></label>
            <button className="button" disabled={busy||!data.costSources.length}>Link cost</button>
          </form>}
          <form
            className="card business-form"
            onSubmit={(e) => {
              e.preventDefault();
              const result = marketingAssetEditorSchema.safeParse({
                kind: assetKind, name: assetName, headline, summary, reference, consentRequired,
              });
              if (!result.success) {
                setError(result.error.issues[0]?.message ?? "Review the asset details.");
                return;
              }
              void send({ entity: "asset", ...marketingAssetDraft(result.data) });
            }}
          >
            <h2>Draft marketing asset</h2>
            <label>
              Kind
              <select name="kind" value={assetKind} onChange={e=>setAssetKind(e.target.value as typeof assetKind)}>
                <option value="form">Form</option>
                <option value="content">Content</option>
                <option value="domain">Domain</option>
                <option value="provider">Provider</option>
              </select>
            </label>
            <label>
              Name
              <input name="name" value={assetName} onChange={e=>setAssetName(e.target.value)} minLength={2} maxLength={160} required />
            </label>
            {assetKind === "form" ? <>
              <p>This form will ask visitors for their name, contact details and request. A separate reviewer must activate it.</p>
              <label>Public headline<input value={headline} onChange={e=>setHeadline(e.target.value)} minLength={2} maxLength={160} required /></label>
              <label>Supporting text<textarea value={summary} onChange={e=>setSummary(e.target.value)} maxLength={500} /></label>
              <label><input type="checkbox" checked={consentRequired} onChange={e=>setConsentRequired(e.target.checked)} required /> Require visitor consent before submitting</label>
            </> : <>
              <label>Evidence or reference<input value={reference} onChange={e=>setReference(e.target.value)} minLength={2} maxLength={160} required placeholder="Document, URL or review reference" /></label>
              <label>What should reviewers know?<textarea value={summary} onChange={e=>setSummary(e.target.value)} minLength={3} maxLength={500} required /></label>
            </>}
            <aside className="card" aria-label="Asset preview">
              <strong>Preview for review</strong>
              <h3>{assetName.trim() || "Asset name"}</h3>
              {assetKind === "form" ? <><p>{headline.trim() || "Public headline"}</p>
                {summary.trim()&&<p>{summary.trim()}</p>}
                <small>{consentRequired ? "Consent required" : "Choose consent before saving"}</small></>
                : <><p>{summary.trim() || "Review summary"}</p><small>{reference.trim() || "Evidence reference"}</small></>}
            </aside>
            <button className="button" disabled={busy}>
              Draft asset
            </button>
          </form>
          {data.assets
            .filter((x) => x.status === "draft")
            .map((x) => (
              <form
                className="card business-form"
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
                <label>
                  Reason
                  <textarea name="reason" required minLength={3} />
                </label>
                <button className="button" disabled={busy}>
                  Activate
                </button>
              </form>
            ))}
          <form
            className="card business-form"
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
                  campaignId: String(f.campaignId ?? ''),
                },
              });
            }}
          >
            <h2>Capture consented lead</h2>
            <label>
              Active form
              <select name="formId">
                {data.assets
                  .filter((x) => x.kind === "form" && x.status === "active")
                  .map((x) => (
                    <option key={String(x.id)} value={String(x.id)}>
                      {String(x.name)}
                    </option>
                  ))}
              </select>
            </label>
            <label>
              Branch
              <select name="branchId">
                {data.branches.map((x) => (
                  <option key={String(x.id)} value={String(x.id)}>
                    {String(x.name)}
                  </option>
                ))}
              </select>
            </label>
            <label>
              Contact name
              <input name="contactName" required />
            </label>
            <label>
              Contact detail
              <textarea name="contactDetail" required minLength={3} />
            </label>
            <label>
              Consent evidence
              <textarea name="consentEvidence" required minLength={3} />
            </label>
            <label>
              Campaign
              <input name="campaign" required />
            </label>
            <label>Reviewed campaign ID
              <select name="campaignId"><option value="">No linked campaign</option>{data.campaigns.map((x)=><option key={String(x.id)} value={String(x.id)}>{String(x.code)} · {String(x.name)}</option>)}</select>
            </label>
            <label>
              Source
              <input name="source" required />
            </label>
            <label>
              Brief
              <textarea name="brief" required minLength={3} />
            </label>
            <button className="button" disabled={busy}>
              Create lead candidate
            </button>
          </form>
        </>
      )}
      {c && c.status === "pending" && (
        <>
          <form
            className="card business-form"
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
            <label>
              Event
              <input name="eventType" required />
            </label>
            <label>
              Channel
              <input name="channel" required />
            </label>
            <label>
              Evidence
              <textarea name="reason" required minLength={3} />
            </label>
            <button className="button" disabled={busy}>
              Record
            </button>
          </form>
          <form
            className="card business-form"
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
            <label>
              Due date
              <input name="dueDate" type="date" required />
            </label>
            <label>
              Review evidence
              <textarea name="reason" required minLength={3} />
            </label>
            <button className="button" disabled={busy}>
              Create CRM lead
            </button>
          </form>
        </>
      )}
    </>
  );
}
