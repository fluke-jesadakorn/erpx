"use client";
import { InlineNotice } from "@/components/ui/foundation";

import { Button, Field, Select, TextArea, TextInput } from "@/components/ui/foundation";

import { useRouter } from "@/lib/shared/destination-router";
import { useHydrated } from "@/components/ui/use-hydrated";
import { useRecoverableDraft } from "@/components/ui/use-recoverable-draft";
import {useHandbookCommand} from './use-handbook-command';
import type {HandbookPurchaseControls} from '@/server/modules/collaboration/public/handbook-purchases';
import {DraftFormDrawer} from '@/components/ui/draft-form-drawer';
import {useEffect,useRef,useState} from 'react';
import {registerUnsavedWork} from '@/lib/work_projection/context-switch';
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
  purchase,
  contextKey = base,
}: {
  base: string;
  companyId: string;
  userId: string;
  id?: string;
  version?: number;
  revision?: HandbookRevision;
  archived?: boolean;
  purchase?:HandbookPurchaseControls;
  contextKey?:string;
}) {
  const ready = useHydrated(),
    router = useRouter(),
    command = useHandbookCommand(base,purchase,contextKey),
    initial = {
      title: revision?.title ?? "",
      category: revision?.category ?? "procedure",
      body: revision?.body ?? "",
    };
  const draft = useRecoverableDraft(
      `erpx:handbook:${companyId}:${userId}:${contextKey}:${id ?? "new"}`,
      initial,
      "article",
    ),
    f = draft.fields,
    dirty = draft.dirty,form=useRef<HTMLFormElement>(null);
  useEffect(()=>registerUnsavedWork(()=>dirty),[dirty]);
  async function save(e: React.FormEvent<HTMLFormElement>) {
    e.preventDefault();
    await command.send(`handbook${id ? `/${id}` : ""}`, {
      ...f,
      expectedVersion: version,
    },e.currentTarget,false,result=>{draft.clear();if(!id)router.push(command.canonicalArticle(result.id));});
  }
  return (
    <Card as="section" appearance="standard" className="ui-panel">
      <h2>{id ? "Edit article" : "New article"}</h2>
      <p className={styles.quiet}>
        Saving keeps a private draft. Publish only after reviewing the saved revision.
      </p>
      {draft.notice}
      <form ref={form} data-handbook-draft data-work-dirty={dirty?'true':undefined} onChangeCapture={event=>event.currentTarget.setAttribute('data-erpx-edit-version',crypto.randomUUID())} onReset={()=>draft.discard()} onSubmit={save}>
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
            <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={draft.conflict||!!purchase&&!purchase.newWork||!!command.retryAttempt}>
              Save draft
            </Button>
            {id && (
              <>
                <Button appearance="standard"
                  variant="secondary"
                  type="button"

                  disabled={dirty || draft.conflict||!!purchase&&!purchase.newWork||!!command.retryAttempt}
                  onClick={() =>
                    void command.send(`handbook/${id}/actions`, {
                      action: "publish",
                      expectedVersion: version,
                    },form.current??undefined)
                  }
                >
                  Publish saved revision
                </Button>
                <Button appearance="standard"
                  variant="ghost"
                  type="button"
                  disabled={dirty||draft.conflict||!!purchase&&!(archived?purchase.newWork:purchase.settleExisting)||!!command.retryAttempt}

                  onClick={() =>
                    void command.send(`handbook/${id}/actions`, {
                      action: archived ? "restore" : "archive",
                      expectedVersion: version,
                    },form.current??undefined,!archived)
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
      {command.notice&&<InlineNotice tone="info">{command.notice}</InlineNotice>}
      {command.retryAttempt&&<Button appearance="standard" disabled={command.busy||!command.retryAllowed} onClick={()=>void command.retry()}>Retry submitted handbook action</Button>}
    </Card>
  );
}
export function HandbookEditors({
  base,
  companyId,
  userId,
  options,
  members,
  purchase,
  contextKey = base,
}: {
  base: string;
  companyId: string;
  userId: string;
  options: Awaited<ReturnType<typeof handbookEditorOptions>>;
  members: { id: string; name: string }[];
  purchase?:HandbookPurchaseControls;
  contextKey?:string;
}) {
  const ready = useHydrated(),
    command = useHandbookCommand(base,purchase,contextKey),
    [open,setOpen]=useState(false),
    draft = useRecoverableDraft(
      `erpx:handbook-editors:${companyId}:${userId}:${contextKey}`,
      { member: "" },
      "editor access",
    );
  const dirty=draft.fields.member!=='';
  useEffect(()=>registerUnsavedWork(()=>open&&dirty),[open,dirty]);
  return (
    <><Button appearance="standard" clientManaged aria-haspopup="dialog" aria-expanded={open} variant="secondary" disabled={!ready||command.busy} onClick={()=>setOpen(true)}>Manage handbook editors</Button><DraftFormDrawer appearance="standard" open={open} dirty={dirty} title="Manage handbook editors" dismissDisabled={command.busy} onClose={()=>setOpen(false)} onDiscard={()=>{draft.discard();setOpen(false);}}>
      {options.editors.map((e) => (
        <div className={styles.actions} key={e.user_id}>
          <p>{members.find((m) => m.id === e.user_id)?.name ?? e.user_id}</p>
          <Button appearance="standard"
            type="submit"
            variant="ghost"

            disabled={!ready || command.busy||!!purchase&&!purchase.settleExisting||!!command.retryAttempt}
            onClick={() =>
              void command.send("handbook/editors", {
                userId: e.user_id,
                active: false,
                expectedVersion: options.version,
              },undefined,true)
            }
          >
            Revoke editor access
          </Button>
        </div>
      ))}
      <form
        onSubmit={async (e) => {
          e.preventDefault();
          await command.send("handbook/editors", {
              userId: draft.fields.member,
              active: true,
              expectedVersion: options.version,
            },e.currentTarget,false,()=>draft.discard());
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
          <Button appearance="standard" clientManaged type="submit" variant="secondary" disabled={!!purchase&&!purchase.newWork||!!command.retryAttempt}>
            Grant editor access
          </Button>
        </fieldset>
      </form>
      {command.error && <InlineNotice tone="danger" role="alert">{command.error}</InlineNotice>}
      {command.retryAttempt&&<Button appearance="standard" disabled={command.busy||!command.retryAllowed} onClick={()=>void command.retry()}>Retry submitted handbook action</Button>}
    </DraftFormDrawer></>
  );
}
