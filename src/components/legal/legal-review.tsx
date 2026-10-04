"use client";
import { InlineNotice } from "@/components/ui/foundation";

import { FormDisclosure } from "@/components/ui/form-disclosure";
import { Button, Field, Select, TextArea, TextInput } from "@/components/ui/foundation";

import { useEffect, useState } from "react";
import { diffWordsWithSpace } from "diff";
import type { LegalDetail } from "@/lib/legal/model";
import s from "./legal.module.css";
import { LegalEvidenceFields } from "./legal-evidence-fields";
import { LegalPeopleSequence, useLegalPeople } from "./legal-person-select";
type Props = {
  detail: LegalDetail;
  call: (path: string, body?: unknown) => Promise<unknown>;
  command: (body: Record<string, unknown>) => Promise<boolean>;
  busy: boolean;
};
export function LegalApproval({
  detail,
  call,
  command,
  busy,
  people,
}: Props & { people: { id: string; name: string }[] }) {
  const { people: reviewers, error: reviewerError } = useLegalPeople(
    call,
    "reviewer",
    detail.item.id,
  );
  const { people: signers, error: signerError } = useLegalPeople(call, "signer", detail.item.id);
  const [approverIds, setApproverIds] = useState([""]);
  const [signerIds, setSignerIds] = useState([""]);
  const [view, setView] = useState<{
      instance: { state: string } | null;
      steps: {
        id: string;
        ordinal: number;
        approver_user_id: string;
        state: string;
      }[];
      history: unknown[];
    } | null>(null),
    [error, setError] = useState("");
  useEffect(() => {
    let live = true;
    call(`/approval/${detail.item.id}`)
      .then((v) => {
        if (live) setView(v as typeof view);
      })
      .catch((e) => {
        if (live) setError(e.message);
      });
    return () => {
      live = false;
    };
  }, [call, detail.item.id, detail.item.version]);
  return (
    <section className={s.detail}>
      <h3>Contract approval route</h3>
      {error && <InlineNotice tone="danger" role="alert">{error}</InlineNotice>}
      {(reviewerError || signerError) && <InlineNotice tone="danger" role="alert">{reviewerError || signerError}</InlineNotice>}
      {view?.steps.map((step) => (
        <p key={step.id}>
          {step.ordinal}.{" "}
          {people.find((person) => person.id === step.approver_user_id)?.name ??
            "Assigned reviewer"}{" "}
          · {step.state}
        </p>
      ))}
      {["draft", "review", "approval"].includes(detail.item.state) && (
        <form
          className={s.detail}
          onSubmit={(e) => {
            e.preventDefault();
            const v = Object.fromEntries(new FormData(e.currentTarget));
            void command({
              action: "approval.open",
              ...v,
              approverIds,
              signerIds,
            });
          }}
        >
          <Field appearance="standard" className={s.field}>
            Document version
            <Select appearance="standard" name="documentId" required>
              {detail.documents.map((d) => (
                <option key={d.id} value={d.id}>
                  {d.name} · v{d.version}
                </option>
              ))}
            </Select>
          </Field>
          <LegalPeopleSequence
            label="Reviewer"
            name="approver"
            people={reviewers}
            values={approverIds}
            onChange={setApproverIds}
          />
          <LegalPeopleSequence
            label="Expected signer"
            name="signer"
            people={signers}
            values={signerIds}
            onChange={setSignerIds}
          />
          <Field appearance="standard" className={s.field}>
            Reason
            <TextInput appearance="standard" name="reason" required minLength={3} />
          </Field>
          <Button appearance="standard"
            clientManaged type="submit"
            variant="ghost"
            className={s.button}
            disabled={
              busy ||
              !detail.documents.length ||
              !approverIds.every(Boolean) ||
              !signerIds.every(Boolean)
            }
          >
            Open approval route
          </Button>
        </form>
      )}
      {view?.instance?.state === "pending" && (
        <>
          <form
            className={s.detail}
            onSubmit={(e) => {
              e.preventDefault();
              void command({
                action: "approval.decide",
                ...Object.fromEntries(new FormData(e.currentTarget)),
              });
            }}
          >
            <Field appearance="standard" className={s.field}>
              Decision
              <Select appearance="standard" name="decision">
                <option value="approve">Approve this locked version</option>
                <option value="revise">Return for revision</option>
                <option value="reject">Reject</option>
              </Select>
            </Field>
            <Field appearance="standard" className={s.field}>
              Reason
              <TextInput appearance="standard" name="reason" required minLength={3} />
            </Field>
            <Button appearance="standard" clientManaged type="submit" variant="ghost" className={s.button} disabled={busy}>
              Record my decision
            </Button>
          </form>
          <form
            className={s.detail}
            onSubmit={(e) => {
              e.preventDefault();
              void command({
                action: "approval.reroute",
                ...Object.fromEntries(new FormData(e.currentTarget)),
              });
            }}
          >
            <Field appearance="standard" className={s.field}>
              Replacement reviewer
              <Select appearance="standard" name="approverId" required>
                <option value="">Choose a reviewer</option>
                {reviewers.map((person) => (
                  <option key={person.id} value={person.id}>
                    {person.name}
                  </option>
                ))}
              </Select>
            </Field>
            <Field appearance="standard" className={s.field}>
              Reason for replacement
              <TextInput appearance="standard" name="reason" required minLength={3} />
            </Field>
            <Button appearance="standard" clientManaged type="submit" variant="ghost" className={s.button} disabled={busy}>
              Replace current reviewer
            </Button>
          </form>
        </>
      )}
      <details>
        <summary>Decision history</summary>
        <LegalEvidenceFields value={view?.history ?? []} />
      </details>
    </section>
  );
}
export function LegalEditor({ detail, command, busy }: Pick<Props, "detail" | "command" | "busy">) {
  const docs = detail.documents.filter((d) => d.body),
    [left, setLeft] = useState(docs[1]?.id ?? docs[0]?.id ?? ""),
    [right, setRight] = useState(docs[0]?.id ?? ""),
    [sections, setSections] = useState([{ id: crypto.randomUUID(), heading: "", body: "" }]);
  const a = docs.find((d) => d.id === left)?.body ?? "",
    b = docs.find((d) => d.id === right)?.body ?? "",
    changes = diffWordsWithSpace(a, b, { timeout: 100, maxEditLength: 10000 });
  return (
    <section className={s.detail}>
      <details>
        <summary>Compare document versions</summary>
        <div className={s.grid}>
          {[
            [left, setLeft],
            [right, setRight],
          ].map(([value, set], i) => (
            <Field appearance="standard" className={s.field} key={i}>
              {i ? "Revised" : "Original"}
              <Select appearance="standard"
                value={value as string}
                onChange={(e) => (set as typeof setLeft)(e.target.value)}
              >
                {docs.map((d) => (
                  <option key={d.id} value={d.id}>
                    {d.name} · v{d.version}
                  </option>
                ))}
              </Select>
            </Field>
          ))}
        </div>
        <div className={s.body}>
          {changes
            ? changes.map((p, i) =>
                p.added ? (
                  <ins key={i}>{p.value}</ins>
                ) : p.removed ? (
                  <del key={i}>{p.value}</del>
                ) : (
                  <span key={i}>{p.value}</span>
                ),
              )
            : "Comparison exceeds the interactive limit. Download both versions to review."}
        </div>
      </details>
      {detail.item.state === "draft" && (
        <FormDisclosure appearance="standard">
          <summary>Draft by clause</summary>
          <form
            className={s.detail}
            onSubmit={async (e) => {
              e.preventDefault();
              const v = new FormData(e.currentTarget);
              if (
                await command({
                  action: "document",
                  name: v.get("name"),
                  body: sections.map((c) => `${c.heading}\n${c.body}`).join("\n\n"),
                })
              )
                setSections([{ id: crypto.randomUUID(), heading: "", body: "" }]);
            }}
          >
            <Field appearance="standard" className={s.field}>
              Version name
              <TextInput appearance="standard" name="name" required />
            </Field>
            {sections.map((c, i) => (
              <fieldset key={c.id}>
                <legend>Clause {i + 1}</legend>
                <Field appearance="standard" className={s.field}>
                  Heading
                  <TextInput appearance="standard"
                    value={c.heading}
                    onChange={(e) =>
                      setSections(
                        sections.map((x) =>
                          x.id === c.id ? { ...x, heading: e.target.value } : x,
                        ),
                      )
                    }
                  />
                </Field>
                <Field appearance="standard" className={s.field}>
                  Text
                  <TextArea appearance="standard"
                    required
                    value={c.body}
                    onChange={(e) =>
                      setSections(
                        sections.map((x) => (x.id === c.id ? { ...x, body: e.target.value } : x)),
                      )
                    }
                  />
                </Field>
                <Button appearance="standard"
                  variant="ghost"
                  type="button"
                  className={s.button}
                  disabled={i === 0}
                  onClick={() => {
                    const next = [...sections];
                    [next[i - 1], next[i]] = [next[i], next[i - 1]];
                    setSections(next);
                  }}
                >
                  Move up
                </Button>
                <Button appearance="standard"
                  variant="ghost"
                  type="button"
                  className={s.button}
                  disabled={sections.length === 1}
                  onClick={() => setSections(sections.filter((x) => x.id !== c.id))}
                >
                  Remove clause
                </Button>
              </fieldset>
            ))}
            <Button appearance="standard"
              variant="ghost"
              type="button"
              className={s.button}
              onClick={() =>
                setSections([...sections, { id: crypto.randomUUID(), heading: "", body: "" }])
              }
            >
              Add clause
            </Button>
            <Button appearance="standard" clientManaged type="submit" variant="ghost" className={s.button} disabled={busy}>
              Save immutable draft version
            </Button>
          </form>
        </FormDisclosure>
      )}
    </section>
  );
}
