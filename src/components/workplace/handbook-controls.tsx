"use client";
import { InlineNotice } from "@/components/ui/foundation";

import { Button, Field, Select, TextArea, TextInput } from "@/components/ui/foundation";

import { useRouter } from "next/navigation";
import { useHydrated } from "@/components/ui/use-hydrated";
import { useRecoverableDraft } from "@/components/ui/use-recoverable-draft";
import { useStaffCommand } from "@/components/modules/staff-work-controls";
import type { HandbookRevision, handbookEditorOptions } from "@/server/modules/runtime/public/handbook";
import styles from "./services.module.css";
import { Card } from "@/components/ui/foundation";
export function HandbookEditor({
  base,
  companyId,
  userId,
  id,
  version = 1,
  revision,
  archived = false,
}: {
  base: string;
  companyId: string;
  userId: string;
  id?: string;
  version?: number;
  revision?: HandbookRevision;
  archived?: boolean;
}) {
  const ready = useHydrated(),
    router = useRouter(),
    command = useStaffCommand(base),
    initial = {
      title: revision?.title ?? "",
      category: revision?.category ?? "procedure",
      body: revision?.body ?? "",
    };
  const draft = useRecoverableDraft(
      `erpx:handbook:${companyId}:${userId}:${id ?? "new"}`,
      initial,
      "article",
    ),
    f = draft.fields,
    dirty = JSON.stringify(f) !== JSON.stringify(initial);
  async function save(e: React.FormEvent) {
    e.preventDefault();
    const result = await command.send(`handbook${id ? `/${id}` : ""}`, {
      ...f,
      expectedVersion: version,
    });
    if (result) {
      draft.clear();
      if (!id) router.push(`${base}/handbook/${result.id}?edit=1`);
    }
  }
  return (
    <Card as="section" appearance="standard" className="ui-panel">
      <h2>{id ? "Edit article" : "New article"}</h2>
      <p className={styles.quiet}>
        Saving keeps a private draft. Publish only after reviewing the saved revision.
      </p>
      {draft.notice}
      <form onSubmit={save}>
        <fieldset className={styles.form} disabled={!ready || command.busy}>
          <Field appearance="standard">
            Article title
            <TextInput appearance="standard"
              maxLength={200}
              value={f.title}
              onChange={(e) => draft.update((p) => ({ ...p, title: e.target.value }))}
            />
          </Field>
          <Field appearance="standard">
            Category
            <Select appearance="standard"
              value={f.category}
              onChange={(e) => draft.update((p) => ({ ...p, category: e.target.value }))}
            >
              <option value="policy">Policy</option>
              <option value="procedure">Procedure</option>
              <option value="faq">FAQ</option>
              <option value="service_guide">Service guidance</option>
            </Select>
          </Field>
          <Field appearance="standard">
            Article content
            <TextArea appearance="standard"
              style={{ minHeight: 280 }}
              maxLength={40000}
              value={f.body}
              onChange={(e) => draft.update((p) => ({ ...p, body: e.target.value }))}
            />
          </Field>
          <div className={styles.actions}>
            <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={draft.conflict}>
              Save draft
            </Button>
            {id && (
              <>
                <Button appearance="standard"
                  variant="secondary"
                  type="button"

                  disabled={dirty || draft.conflict}
                  onClick={() =>
                    void command.send(`handbook/${id}/actions`, {
                      action: "publish",
                      expectedVersion: version,
                    })
                  }
                >
                  Publish saved revision
                </Button>
                <Button appearance="standard"
                  variant="ghost"
                  type="button"

                  onClick={() =>
                    void command.send(`handbook/${id}/actions`, {
                      action: archived ? "restore" : "archive",
                      expectedVersion: version,
                    })
                  }
                >
                  {archived ? "Restore article" : "Archive article"}
                </Button>
              </>
            )}
          </div>
        </fieldset>
      </form>
      {command.error && <InlineNotice tone="danger" role="alert">{command.error}</InlineNotice>}
    </Card>
  );
}
export function HandbookEditors({
  base,
  companyId,
  userId,
  options,
  members,
}: {
  base: string;
  companyId: string;
  userId: string;
  options: Awaited<ReturnType<typeof handbookEditorOptions>>;
  members: { id: string; name: string }[];
}) {
  const ready = useHydrated(),
    command = useStaffCommand(base),
    draft = useRecoverableDraft(
      `erpx:handbook-editors:${companyId}:${userId}`,
      { member: "" },
      "editor access",
    );
  return (
    <details className={`card ${styles.details}`}>
      <summary>Manage handbook editors</summary>
      {options.editors.map((e) => (
        <div className={styles.actions} key={e.user_id}>
          <p>{members.find((m) => m.id === e.user_id)?.name ?? e.user_id}</p>
          <Button appearance="standard"
            type="submit"
            variant="ghost"

            disabled={!ready || command.busy}
            onClick={() =>
              void command.send("handbook/editors", {
                userId: e.user_id,
                active: false,
                expectedVersion: options.version,
              })
            }
          >
            Revoke editor access
          </Button>
        </div>
      ))}
      <form
        onSubmit={async (e) => {
          e.preventDefault();
          if (
            await command.send("handbook/editors", {
              userId: draft.fields.member,
              active: true,
              expectedVersion: options.version,
            })
          )
            draft.clear();
        }}
      >
        <fieldset className={styles.form} disabled={!ready || command.busy}>
          <Field appearance="standard">
            Company member
            <Select appearance="standard"
              value={draft.fields.member}
              required
              onChange={(e) => draft.update((p) => ({ ...p, member: e.target.value }))}
            >
              <option value="">Choose member</option>
              {members
                .filter((m) => !options.editors.some((e) => e.user_id === m.id))
                .map((m) => (
                  <option key={m.id} value={m.id}>
                    {m.name}
                  </option>
                ))}
            </Select>
          </Field>
          <Button appearance="standard" clientManaged type="submit" variant="secondary">
            Grant editor access
          </Button>
        </fieldset>
      </form>
      {command.error && <InlineNotice tone="danger" role="alert">{command.error}</InlineNotice>}
    </details>
  );
}
