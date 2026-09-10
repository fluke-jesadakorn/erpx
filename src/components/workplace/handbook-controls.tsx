"use client";
import { useRouter } from "next/navigation";
import { useHydrated } from "@/components/ui/use-hydrated";
import { useRecoverableDraft } from "@/components/ui/use-recoverable-draft";
import { useStaffCommand } from "@/components/modules/staff-work-controls";
import type {
  HandbookRevision,
  handbookEditorOptions,
} from "@/server/handbook";
import styles from "./services.module.css";
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
    <section className="card">
      <h2>{id ? "Edit article" : "New article"}</h2>
      <p className={styles.quiet}>
        Saving keeps a private draft. Publish only after reviewing the saved
        revision.
      </p>
      {draft.notice}
      <form onSubmit={save}>
        <fieldset className={styles.form} disabled={!ready || command.busy}>
          <label>
            Article title
            <input
              maxLength={200}
              value={f.title}
              onChange={(e) =>
                draft.update((p) => ({ ...p, title: e.target.value }))
              }
            />
          </label>
          <label>
            Category
            <select
              value={f.category}
              onChange={(e) =>
                draft.update((p) => ({ ...p, category: e.target.value }))
              }
            >
              <option value="policy">Policy</option>
              <option value="procedure">Procedure</option>
              <option value="faq">FAQ</option>
              <option value="service_guide">Service guidance</option>
            </select>
          </label>
          <label>
            Article content
            <textarea
              style={{ minHeight: 280 }}
              maxLength={40000}
              value={f.body}
              onChange={(e) =>
                draft.update((p) => ({ ...p, body: e.target.value }))
              }
            />
          </label>
          <div className={styles.actions}>
            <button className="button" disabled={draft.conflict}>
              Save draft
            </button>
            {id && (
              <>
                <button
                  type="button"
                  className="button secondary"
                  disabled={dirty || draft.conflict}
                  onClick={() =>
                    void command.send(`handbook/${id}/actions`, {
                      action: "publish",
                      expectedVersion: version,
                    })
                  }
                >
                  Publish saved revision
                </button>
                <button
                  type="button"
                  className="button ghost"
                  onClick={() =>
                    void command.send(`handbook/${id}/actions`, {
                      action: archived ? "restore" : "archive",
                      expectedVersion: version,
                    })
                  }
                >
                  {archived ? "Restore article" : "Archive article"}
                </button>
              </>
            )}
          </div>
        </fieldset>
      </form>
      {command.error && <p role="alert">{command.error}</p>}
    </section>
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
          <button
            className="button ghost"
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
          </button>
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
          <label>
            Company member
            <select
              value={draft.fields.member}
              required
              onChange={(e) =>
                draft.update((p) => ({ ...p, member: e.target.value }))
              }
            >
              <option value="">Choose member</option>
              {members
                .filter((m) => !options.editors.some((e) => e.user_id === m.id))
                .map((m) => (
                  <option key={m.id} value={m.id}>
                    {m.name}
                  </option>
                ))}
            </select>
          </label>
          <button className="button secondary">Grant editor access</button>
        </fieldset>
      </form>
      {command.error && <p role="alert">{command.error}</p>}
    </details>
  );
}
