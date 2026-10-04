"use client";
import { InlineNotice } from "@/components/ui/foundation";

import { FormDisclosure } from "@/components/ui/form-disclosure";
import { Button, Field, Select, TextArea, TextInput } from "@/components/ui/foundation";

import { useCallback, useEffect, useState } from "react";
import type { LegalData, LegalDetail } from "@/lib/legal/model";
import s from "./legal.module.css";

type Row = {
  id: string;
  rule_id: string;
  source_id?: string;
  revision_id?: string;
  state: string;
  version: number;
  created_by: string;
  title?: string;
  record_title: string;
  source_url?: string;
  revision_label?: string;
  published_on?: string;
  effective_from?: string;
  effective_until?: string;
  applies?: boolean;
  basis?: string;
  check_date?: string;
  error?: string;
  checked_at?: number;
};
type Corpus = {
  sources: Row[];
  revisions: Row[];
  applicability: Row[];
  checks: Row[];
  notice: string;
};
export function LegalLawCorpus({
  call,
  manage,
  canReview,
  userId,
}: {
  call: (path: string, body?: unknown) => Promise<unknown>;
  manage: boolean;
  canReview: boolean;
  userId: string;
}) {
  const [data, setData] = useState<Corpus | null>(null);
  const [rules, setRules] = useState<LegalData["items"]>([]);
  const [selected, setSelected] = useState<LegalDetail | null>(null);
  const [ruleId, setRuleId] = useState("");
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const [chunkCount, setChunkCount] = useState(1);
  const refresh = useCallback(async () => {
    const [corpus, register] = await Promise.all([
      call(`/laws${ruleId ? `?ruleId=${encodeURIComponent(ruleId)}` : ""}`),
      call("?kind=rule"),
    ]);
    setData(corpus as Corpus);
    setRules((register as LegalData).items);
    if (ruleId) setSelected((await call(`/${ruleId}`)) as LegalDetail);
  }, [call, ruleId]);
  useEffect(() => {
    let active = true;
    queueMicrotask(() => {
      if (active)
        void refresh().catch((error) => {
          if (active) setError(error.message);
        });
    });
    return () => {
      active = false;
    };
  }, [refresh]);
  async function command(action: string, values: Record<string, unknown>) {
    if (!selected) return;
    setBusy(true);
    setError("");
    try {
      await call("", {
        action,
        id: selected.item.id,
        expectedVersion: selected.item.version,
        ...values,
      });
      await refresh();
    } catch (error) {
      setError(error instanceof Error ? error.message : "Unable to record source review.");
    } finally {
      setBusy(false);
    }
  }
  return (
    <div className={s.detail}>
      <p>
        {data?.notice ??
          "Law text, dates and Company applicability remain pending until independently reviewed."}
      </p>
      <p className={s.small}>
        Dates use the Gregorian calendar. A daily check runs after 06:00 Asia/Bangkok; download or
        OCR failures require manual review of the official original.
      </p>
      {error && <InlineNotice tone="danger" role="alert">{error}</InlineNotice>}
      <Field appearance="standard" className={s.field}>
        Jurisdiction rule
        <Select appearance="standard"
          value={ruleId}
          onChange={(event) => {
            setRuleId(event.target.value);
            setSelected(null);
          }}
        >
          <option value="">All accessible law sources</option>
          {rules.map((rule) => (
            <option key={rule.id} value={rule.id}>
              {rule.title}
            </option>
          ))}
        </Select>
      </Field>
      {!rules.length && (
        <p>
          Create a jurisdiction rule in the register, attach its official original, then register a
          source here.
        </p>
      )}
      {data?.sources.map((source) => (
        <article key={source.id} className={s.event}>
          <strong>{source.title ?? source.record_title}</strong>
          <p>
            <a href={source.source_url} target="_blank" rel="noreferrer">
              Official source
            </a>{" "}
            · {source.state}
          </p>
        </article>
      ))}
      {selected && manage && (
        <details className={s.event}>
          <summary>Register an official source</summary>
          <form
            className={s.detail}
            onSubmit={(event) => {
              event.preventDefault();
              void command(
                "law.source.register",
                Object.fromEntries(new FormData(event.currentTarget)),
              );
            }}
          >
            <Field appearance="standard" className={s.field}>
              Source title
              <TextInput appearance="standard" name="title" required minLength={3} maxLength={300} />
            </Field>
            <Field appearance="standard" className={s.field}>
              Official HTTPS URL
              <TextInput appearance="standard" name="sourceUrl" type="url" required />
            </Field>
            <Reason />
            <Button appearance="standard" clientManaged type="submit" variant="ghost" className={s.button} disabled={busy}>
              Register official source
            </Button>
          </form>
        </details>
      )}
      {selected && manage && (
        <FormDisclosure appearance="standard" className={s.event}>
          <summary>Import a law revision for independent review</summary>
          <form
            className={s.detail}
            onSubmit={(event) => {
              event.preventDefault();
              const values = new FormData(event.currentTarget);
              void command("law.revision.import", {
                sourceId: values.get("sourceId"),
                documentId: values.get("documentId"),
                revisionLabel: values.get("revisionLabel"),
                publishedOn: values.get("publishedOn"),
                effectiveFrom: values.get("effectiveFrom"),
                ...(values.get("effectiveUntil")
                  ? { effectiveUntil: values.get("effectiveUntil") }
                  : {}),
                extractionMethod: values.get("extractionMethod"),
                ...(values.get("inspectionId") ? { inspectionId: values.get("inspectionId") } : {}),
                reason: values.get("reason"),
                chunks: Array.from({ length: chunkCount }, (_, index) => ({
                  page: Number(values.get(`page:${index}`)),
                  section: String(values.get(`section:${index}`) ?? ""),
                  text: String(values.get(`text:${index}`)),
                })),
              });
            }}
          >
            <Field appearance="standard" className={s.field}>
              Registered official source
              <Select appearance="standard" name="sourceId" required>
                {data?.sources
                  .filter((source) => source.rule_id === ruleId)
                  .map((source) => (
                    <option key={source.id} value={source.id}>
                      {source.title}
                    </option>
                  ))}
              </Select>
            </Field>
            <Field appearance="standard" className={s.field}>
              Immutable official original
              <Select appearance="standard" name="documentId" required>
                {selected.documents.map((document) => (
                  <option key={document.id} value={document.id}>
                    {document.name} · v{document.version}
                  </option>
                ))}
              </Select>
            </Field>
            <Field appearance="standard" className={s.field}>
              Revision label
              <TextInput appearance="standard" name="revisionLabel" required maxLength={200} />
            </Field>
            <div className={s.grid}>
              <Field appearance="standard" className={s.field}>
                Publication date
                <TextInput appearance="standard" name="publishedOn" type="date" required />
              </Field>
              <Field appearance="standard" className={s.field}>
                Effective from
                <TextInput appearance="standard" name="effectiveFrom" type="date" required />
              </Field>
              <Field appearance="standard" className={s.field}>
                Effective until, exclusive
                <TextInput appearance="standard" name="effectiveUntil" type="date" />
              </Field>
            </div>
            <Field appearance="standard" className={s.field}>
              Text extraction
              <Select appearance="standard" name="extractionMethod">
                <option value="manual">Manual transcription checked against original</option>
                <option value="ocr">OCR inspection of this original</option>
              </Select>
            </Field>
            <Field appearance="standard" className={s.field}>
              OCR inspection reference, when using OCR
              <TextInput appearance="standard" name="inspectionId" />
            </Field>
            {Array.from({ length: chunkCount }, (_, index) => (
              <fieldset key={index} className={s.detail}>
                <legend>Source passage {index + 1}</legend>
                <Field appearance="standard" className={s.field}>
                  Page
                  <TextInput appearance="standard" type="number" name={`page:${index}`} min={1} max={10000} required />
                </Field>
                <Field appearance="standard" className={s.field}>
                  Section or provision
                  <TextInput appearance="standard" name={`section:${index}`} maxLength={200} />
                </Field>
                <Field appearance="standard" className={s.field}>
                  Text from the official original
                  <TextArea appearance="standard" name={`text:${index}`} required maxLength={12000} />
                </Field>
              </fieldset>
            ))}
            <Button appearance="standard"
              variant="ghost"
              type="button"
              className={s.button}
              disabled={chunkCount >= 300}
              onClick={() => setChunkCount(chunkCount + 1)}
            >
              Add source passage
            </Button>
            <Reason />
            <Button appearance="standard"
              clientManaged type="submit"
              variant="ghost"
              className={s.button}
              disabled={
                busy ||
                !selected.documents.length ||
                !data?.sources.some((source) => source.rule_id === ruleId)
              }
            >
              Save pending law revision
            </Button>
          </form>
        </FormDisclosure>
      )}
      {data?.revisions.map((revision) => (
        <article className={s.notice} key={revision.id}>
          <h3>
            {revision.record_title} · {revision.revision_label}
          </h3>
          <p>
            {revision.state} · Published {revision.published_on} · Effective{" "}
            {revision.effective_from}
            {revision.effective_until ? ` until ${revision.effective_until}` : ""}
          </p>
          {selected?.item.id === revision.rule_id &&
            canReview &&
            revision.state === "pending" &&
            revision.created_by !== userId && (
              <form
                className={s.detail}
                onSubmit={(event) => {
                  event.preventDefault();
                  const values = new FormData(event.currentTarget);
                  void command("law.revision.approve", {
                    revisionId: revision.id,
                    revisionVersion: revision.version,
                    originalVerified: values.has("originalVerified"),
                    datesVerified: values.has("datesVerified"),
                    textVerified: values.has("textVerified"),
                    reason: values.get("reason"),
                  });
                }}
              >
                {[
                  ["originalVerified", "Original and official publication verified"],
                  ["datesVerified", "Publication, effective and amendment dates verified"],
                  ["textVerified", "Every page and provision checked against the original"],
                ].map(([key, label]) => (
                  <Field appearance="standard" orientation="inline" className={s.check} key={key}>
                    <TextInput appearance="standard" name={key} type="checkbox" required />
                    {label}
                  </Field>
                ))}
                <Reason />
                <Button appearance="standard" clientManaged type="submit" variant="ghost" className={s.button} disabled={busy}>
                  Approve law revision independently
                </Button>
              </form>
            )}
          {selected?.item.id === revision.rule_id && manage && revision.state === "approved" && (
            <form
              className={s.detail}
              onSubmit={(event) => {
                event.preventDefault();
                const values = new FormData(event.currentTarget);
                void command("law.applicability.propose", {
                  revisionId: revision.id,
                  applies: values.get("applies") === "true",
                  basis: values.get("basis"),
                  reason: values.get("reason"),
                });
              }}
            >
              <Field appearance="standard" className={s.field}>
                Company applicability
                <Select appearance="standard" name="applies">
                  <option value="true">Applies to this Company</option>
                  <option value="false">Does not apply to this Company</option>
                </Select>
              </Field>
              <Field appearance="standard" className={s.field}>
                Applicability assessment
                <TextArea appearance="standard" name="basis" required minLength={10} maxLength={12000} />
              </Field>
              <Reason />
              <Button appearance="standard" clientManaged type="submit" variant="ghost" className={s.button} disabled={busy}>
                Propose applicability for independent review
              </Button>
            </form>
          )}
        </article>
      ))}
      {data?.applicability.map((assessment) => (
        <article className={s.event} key={assessment.id}>
          <strong>
            {assessment.applies ? "Applicable" : "Not applicable"} · {assessment.state}
          </strong>
          <p>{assessment.basis}</p>
          {selected?.item.id === assessment.rule_id &&
            canReview &&
            assessment.state === "pending" &&
            assessment.created_by !== userId && (
              <form
                className={s.detail}
                onSubmit={(event) => {
                  event.preventDefault();
                  void command("law.applicability.approve", {
                    applicabilityId: assessment.id,
                    applicabilityVersion: assessment.version,
                    reason: new FormData(event.currentTarget).get("reason"),
                  });
                }}
              >
                <Reason />
                <Button appearance="standard" clientManaged type="submit" variant="ghost" className={s.button} disabled={busy}>
                  Approve Company applicability independently
                </Button>
              </form>
            )}
        </article>
      ))}
      <section className={s.detail}>
        <h3>Official source checks</h3>
        {!data?.checks.length && <p>No source check has completed.</p>}
        {data?.checks.map((check) => (
          <article className={s.event} key={check.id}>
            <strong>{check.record_title}</strong>
            <p>
              {check.check_date} · {check.state.replaceAll("_", " ")}
            </p>
            {check.error && (
              <InlineNotice tone="info" role="status">
                Source check failed: {check.error}. Import and review the official original
                manually.
              </InlineNotice>
            )}
          </article>
        ))}
      </section>
    </div>
  );
}
function Reason() {
  return (
    <Field appearance="standard" className={s.field}>
      Reason and evidence
      <TextArea appearance="standard" name="reason" required minLength={3} maxLength={4000} />
    </Field>
  );
}
