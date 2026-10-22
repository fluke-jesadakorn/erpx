"use client";
import { Button, Field, Select, TextArea, TextInput } from "@erpx/ui/components/foundation";

import { TimeText } from "@erpx/feature-people_payroll/components/time/time-preference-context";
import { useLayoutEffect, useRef, useState, type FormEvent, type ComponentProps } from "react";
import { useRouter } from "@erpx/feature-shared/lib/destination-router";
import Link from "@erpx/workplace/components/workspace-link";
import { FormDisclosure } from "@erpx/ui/components/form-disclosure";
import { DraftFormDrawer } from "@erpx/ui/components/draft-form-drawer";
import { useWorkspaceRouteState } from "@erpx/feature-shared/lib/workspace-route-state";
import {
  directoryChangeInput,
  windowsPolicyDefinition,
  type DirectoryAction,
  type DirectoryAutomationData,
  type DirectoryPreview,
  type DirectoryChangeInput,
  type WindowsPolicyDefinition,
} from "@erpx/feature-identity_access/lib/directory-operations";
import type { AttributeDefinition } from "@erpx/feature-identity_access/lib/user-management";
import {StandardDirectoryConsole} from '@erpx/feature-identity_access/components/technology/directory-console-standard';
import {DirectoryDraftBoundary} from '@erpx/feature-identity_access/components/technology/directory-draft-boundary';
import {createDirectoryWorkspaceCommands,validDirectoryPreview,type DirectoryWorkspaceSource} from '@erpx/feature-identity_access/lib/directory-workspace-command';
import standardStyles from "@erpx/feature-identity_access/components/technology/directory-console-standard.module.css";

const labels: Record<DirectoryAction["kind"], string> = {
  "account.create": "Create account",
  "account.update": "Update account fields",
  "account.disable": "Disable directory account",
  "account.enable": "Activate reviewed account",
  "group.create": "Create directory group",
  "ou.create": "Create OU",
  "ou.update": "Edit OU",
  "gpo.order": "Reorder GPO links",
  "gpo.inheritance": "Set policy inheritance",
  "gpo.filter": "Security filtering",
  "group.members": "Change group members",
  "object.move": "Move to OU",
  "gpo.publish": "Publish Windows policy",
  "gpo.link": "Link GPO to OU",
};
type DirectoryMappedFieldPreview = {previewToken:string;personName:string;changes:{label:string;from:unknown;to:unknown}[]};
function validMappedFieldPreview(value:unknown):value is DirectoryMappedFieldPreview {
  if (!value || typeof value !== "object" || Array.isArray(value)) return false;
  return "previewToken" in value && typeof value.previewToken === "string" && value.previewToken.length > 0 && "personName" in value && typeof value.personName === "string" && "changes" in value && Array.isArray(value.changes) && value.changes.every(change => !!change && typeof change === "object" && !Array.isArray(change) && "label" in change && typeof change.label === "string" && Object.hasOwn(change,"from") && Object.hasOwn(change,"to"));
}
export function StandardDirectoryWorkspace(props: ComponentProps<typeof StandardDirectoryWorkspaceContent>) {
  const {query} = useWorkspaceRouteState();
  const canonical = props.embedded && query.get("view") === "directory";
  const connectionId = canonical ? query.get("connection") ?? props.initialConnectionId : props.initialConnectionId;
  return <StandardDirectoryWorkspaceContent key={"standard-directory"} {...props} initialConnectionId={connectionId}/>;
}

function StandardDirectoryWorkspaceContent({ appearance = "standard",
  base,
  embedded = false,
  section,
  initial,
  mode = "inventory",
  attributes = [],
  people = [],
  initialConnectionId,
  initialObjectId,
}: ({
  embedded?: boolean;
  section?: "objects" | "mappings" | "work";
  base: string;
  initialConnectionId?: string;
  initialObjectId?: string;
  initial: DirectoryAutomationData;
  mode?: "inventory" | "gpo" | "settings" | "groups";
  attributes?: AttributeDefinition[];
  people?: { id: string; name: string }[];
}) & { appearance?: "standard" }) {
  const {query, update} = useWorkspaceRouteState();
  const canonical = embedded && query.get("view") === "directory";
  const styles=standardStyles,compactStyles=standardStyles;
  const router = useRouter();
  const actionMenu = useRef<HTMLDetailsElement>(null);
  const [localConnectionId, setConnectionId] = useState(
      initialConnectionId ?? initial.connections[0]?.id ?? "",
    ),
    [busy, setBusy] = useState(false),
    [error, setError] = useState(""),
    [notice, setNotice] = useState("");
  const [edit, setEdit] = useState<{
      kind: DirectoryAction["kind"];
      objectId?: string;
      parentId?: string;
      memberId?: string;
      action?: DirectoryAction;
    } | null>(null),
    [request, setRequest] = useState<DirectoryChangeInput | null>(null),
    [preview, setPreview] = useState<DirectoryPreview | null>(null);
  const [policy, setPolicy] = useState<WindowsPolicyDefinition>(() =>
    windowsPolicyDefinition.parse({
      name: "Windows policy",
      scope: "user",
      targetOuId: "unselected",
      filterGroupId: "unselected",
    }),
  );
  const connectionId=canonical?initialConnectionId??localConnectionId:localConnectionId;
  const ObjectConsole=StandardDirectoryConsole;
  const connection = initial.connections.find((c) => c.id === connectionId),
    objects = connection?.inventory?.objects ?? [],
    ous = objects.filter((o) => o.kind === "ou"),
    groups = objects.filter((o) => o.kind === "group");
  const existingLinks = (objects.find((o) => o.id === edit?.parentId)?.properties.gpoLinks ??
    []) as {
    gpoId: string;
    enabled: boolean;
    enforced: boolean;
  }[];
  const existingLink = existingLinks.find((l) => l.gpoId === edit?.objectId);
  const [localSearch, setLocalSearch] = useState("");
  const search = canonical ? query.get("q") ?? "" : localSearch;
  const setSearch = (value: string) => canonical ? update({q:value || null},true) : setLocalSearch(value);
  const [editDirty, setEditDirty] = useState(false);
  const [fieldPreview, setFieldPreview] = useState<{
    input: {
      connectionId: string;
      personId: string;
      direction: string;
      reason: string;
    };
    previewToken: string;
    personName: string;
    changes: { label: string; from: unknown; to: unknown }[];
  } | null>(null);
  const [mappings, setMappings] = useState<
    { attributeId: string; field: string; direction: "read" | "write" }[]
  >(initial.connections[0]?.mappings ?? []);
  const standardSource=useRef<DirectoryWorkspaceSource|null>(null),standardCommands=useRef<ReturnType<typeof createDirectoryWorkspaceCommands>|null>(null),draftRevision=useRef(0),standardBusy=useRef(false);
  const [editorClosed,setEditorClosed]=useState(false),[mappingsOpen,setMappingsOpen]=useState(false),[fieldsOpen,setFieldsOpen]=useState(false),[retainedSurface,setRetainedSurface]=useState<"edit"|"mappings"|"fields"|null>(null);
  const [editConnection,setEditConnection]=useState<string|null>(null),[standardPending,setStandardPending]=useState(false),[standardDirty,setStandardDirty]=useState(false),[standardSaved,setStandardSaved]=useState<{base:string;connectionId:string;path:string;id:string|null}|null>(null);
  useLayoutEffect(()=>{standardSource.current={base,connectionId,source:JSON.stringify([connection,initial.canManage,initial.canApply,initial.canReview,initial.jobs.filter(job=>job.connection_id===connectionId)]),draft:draftRevision.current};standardCommands.current??=createDirectoryWorkspaceCommands(()=>standardSource.current?{...standardSource.current,draft:draftRevision.current}:null);return()=>{standardSource.current=null;};});
  function markStandardDraft(event:FormEvent<HTMLElement>){if(!(event.target as HTMLElement).closest('form, .form-disclosure-content'))return;draftRevision.current++;setStandardDirty(true);const form=(event.target as HTMLElement).closest('form');form?.setAttribute('data-erpx-edit-version',String(draftRevision.current));}
  async function standardSend(path?:string,input?:unknown){
    const command=standardCommands.current;if(!command||standardBusy.current)return;standardBusy.current=true;setBusy(true);setError('');
    try{
      const form=typeof document==='undefined'?null:document.activeElement?.closest('form'),result=await(path?command.submit(path,input,form?{form,formEditVersion:form.getAttribute('data-erpx-edit-version')}:undefined):command.retry());
      if(!result.acknowledge())return;const unchanged=result.unchanged();setStandardPending(false);
      if(result.path!=='preview'&&result.path!=='fields/preview'){setStandardSaved({base,connectionId,path:result.path,id:typeof result.data.id==='string'?result.data.id:null});if(unchanged){setStandardDirty(false);setEditDirty(false);}setNotice(unchanged?'The original directory request is confirmed.':'The original directory request is confirmed. Your newer draft is retained.');router.refresh();}
      else if(!unchanged)setNotice('The preview belongs to an earlier draft. Preview your newer changes before continuing.');
      if(!unchanged)return;
      if(!path){if(result.path==='preview'&&validDirectoryPreview(result.data)){setRequest(directoryChangeInput.parse(result.input));setPreview(result.data);}else if(result.path==='fields/preview'){setNotice('The original mapped-field preview is confirmed. Preview mapped fields again to review the current draft.');}else if(result.path==='apply'||result.path==='submit'){setEdit(null);}else if(result.path==='fields/sync'){setFieldPreview(null);}return;}
      return result.data;
    }catch(failure){setError(failure instanceof Error?failure.message:'Unable to confirm the directory request.');setStandardPending(command.pending());}finally{standardBusy.current=false;setBusy(false);}
  }
  const recovery=<div data-ui-appearance="standard">{notice&&<p role="status">{notice}</p>}{standardPending&&<div role="status"><p>The original directory request is unresolved. Your original request and newer draft are retained.</p><Button appearance="standard" type="button" disabled={busy} onClick={()=>void standardSend()}>Recover original directory request</Button></div>}{standardSaved?.base===base&&standardSaved.connectionId===connectionId&&<p role="status">Confirmed original directory request{standardSaved.id?`: ${standardSaved.id}`:''}. <Link href={`${base}/administration/technology/directory/${standardSaved.path==='fields/sync'?'mappings':'work'}?connection=${encodeURIComponent(connectionId)}`}>View current directory work</Link></p>}</div>;
  async function send(path: string, input: unknown) {if(!standardCommands.current?.pending()&&!standardBusy.current){if(path==="configure")setRetainedSurface("mappings");else if(path==="fields/sync")setRetainedSurface("fields");else if(path==="apply"||path==="submit")setRetainedSurface("edit");else if(path!=="preview"&&path!=="fields/preview")setRetainedSurface(null);}return standardSend(path,input);}
  function open(value: NonNullable<typeof edit>) {
    if(standardCommands.current?.pending()){setError('Recover the original directory request before opening another change. Your draft is retained.');return;}
    setEditorClosed(false);
    setEditConnection(connectionId);
    if (value.kind === "gpo.publish") {
      const saved = value.objectId
        ? objects.find((o) => o.id === value.objectId)?.properties.definition
        : null;
      setPolicy(
        windowsPolicyDefinition.parse(
          saved ?? {
            name: objects.find((o) => o.id === value.objectId)?.name ?? "Windows policy",
            scope: "user",
            targetOuId: "unselected",
            filterGroupId: "unselected",
          },
        ),
      );
    }
    setEditDirty(false);
    setEdit(value);
    setPreview(null);
    setRequest(null);
    setError("");
  }
  function drop(raw: string, target: string, targetKind: string) {
    try {
      const value = JSON.parse(raw),
        source = objects.find((o) => o.id === value.id);
      if (value.connectionId !== connectionId || !source)
        throw Error("Choose objects from the same directory connection.");
      if (targetKind === "group" && ["account", "computer", "group"].includes(source.kind))
        open({ kind: "group.members", objectId: target, memberId: source.id });
      else if (
        targetKind === "ou" &&
        ["account", "computer", "group", "ou", "gpo"].includes(source.kind)
      )
        open({
          kind: source.kind === "gpo" ? "gpo.link" : "object.move",
          objectId: source.id,
          parentId: target,
        });
      else throw Error("This drop is not supported. Choose an action from the object.");
    } catch (e) {
      setError(e instanceof Error ? e.message : "Choose a supported drop.");
    }
  }
  async function review(e: FormEvent<HTMLFormElement>) {
    e.preventDefault();
    if (!edit || !connection) return;
    if(editConnection!==connectionId){setError('Return to the original directory connection to continue this draft.');return;}
    const f = new FormData(e.currentTarget),
      str = (key: string) => String(f.get(key) ?? "");
    let action: DirectoryAction;
    if (edit.action) action = edit.action;
    else if (edit.kind === "ou.update")
      action = {
        kind: edit.kind,
        objectId: str("objectId"),
        description: str("description"),
      };
    else if (edit.kind === "gpo.inheritance")
      action = {
        kind: edit.kind,
        objectId: str("objectId"),
        blocked: f.has("blocked"),
      };
    else if (edit.kind === "gpo.filter")
      action = {
        kind: edit.kind,
        objectId: str("objectId"),
        groupIds: f.getAll("groupIds").map(String),
      };
    else if (edit.kind === "account.create")
      action = {
        kind: edit.kind,
        username: str("username"),
        firstName: str("firstName"),
        lastName: str("lastName"),
        parentId: str("parentId"),
      };
    else if (edit.kind === "group.create" || edit.kind === "ou.create")
      action = {
        kind: edit.kind,
        name: str("name"),
        parentId: str("parentId"),
      };
    else if (edit.kind === "account.disable")
      action = {
        kind: edit.kind,
        objectId: str("objectId"),
        caseId: str("caseId") || null,
      };
    else if (edit.kind === "account.enable")
      action = {
        kind: edit.kind,
        objectId: str("objectId"),
        caseId: str("caseId"),
      };
    else if (edit.kind === "account.update")
      action = {
        kind: edit.kind,
        objectId: str("objectId"),
        fields: { [str("field")]: str("value") },
      };
    else if (edit.kind === "group.members")
      action = {
        kind: edit.kind,
        objectId: str("objectId"),
        memberIds: f.getAll("memberIds").map(String),
        action: str("action") as "add" | "remove",
      };
    else if (edit.kind === "gpo.publish")
      action = {
        kind: edit.kind,
        objectId: str("objectId") || null,
        definition: policy,
      };
    else if (edit.kind === "gpo.link")
      action = {
        kind: edit.kind,
        objectId: str("objectId"),
        parentId: str("parentId"),
        enabled: f.has("enabled"),
        enforced: f.has("enforced"),
        remove: f.has("remove"),
        order: Number(str("order")) || undefined,
      };
    else
      action = {
        kind: "object.move",
        objectId: str("objectId"),
        parentId: str("parentId"),
      };
    const parsed = directoryChangeInput.safeParse({
      connectionId,
      expectedVersion: connection.version,
      action,
      reason: str("reason"),
      affectedCompanyIds: [],
    });
    if (!parsed.success) {
      setError(parsed.error.issues[0]?.message ?? "Check the form.");
      return;
    }
    const result = await send("preview", parsed.data);
    if (result && validDirectoryPreview(result)) {
      setRequest(parsed.data);
      setPreview(result);
    }
  }
  const options = (items: typeof objects) =>
    items.map((o) => (
      <option data-ui-appearance={appearance} key={o.id} value={o.id}>
        {o.name} · {o.kind}
      </option>
    ));
  const actionKinds: DirectoryAction["kind"][] =
    mode === "gpo"
      ? ["gpo.publish"]
      : mode === "groups"
        ? ["group.create"]
        : [
            "account.create",
            "group.create",
            "ou.create",
            "gpo.publish",
            "object.move",
            "group.members",
          ];
  const primaryActions = mode === "inventory" ? actionKinds.slice(0, 2) : actionKinds;
  const moreActions = mode === "inventory" ? actionKinds.slice(2) : [];
  const content=(
    <section onInputCapture={markStandardDraft} onChangeCapture={markStandardDraft} onClickCapture={event=>{if((event.target as HTMLElement).closest('form button[type="button"], button[data-directory-draft-action]'))markStandardDraft(event);}} data-ui-appearance={appearance}
      className={
        (embedded ? "" : "ui-panel ") + styles.stack + (mode === "settings" ? "" : " " + compactStyles.workspace)
      }
    >
      {!embedded && mode !== "inventory" && (
        <h2 data-ui-appearance={appearance}>
          {mode === "gpo"
            ? "Windows policies"
            : mode === "settings"
              ? "Directory write-back & field mapping"
              : mode === "groups"
                ? "Directory groups"
                : "Directory accounts, computers & OUs"}
        </h2>
      )}
      {!connection ? (
        <p data-ui-appearance={appearance}>
          No active UCS or AD connection.{" "}
          <Link href={base + "/technology?view=directory&section=connections"}>Configure a connection</Link>.
        </p>
      ) : (
        <>
          <div data-ui-appearance={appearance} className={mode === "settings" ? styles.stack : compactStyles.workspaceHeader}>
            <Field appearance={appearance}>
              {mode === "settings" ? "Directory connection" : "Connection"}
              <Select appearance={appearance}
                value={connectionId}
                disabled={(busy||standardPending)}
                onChange={(e) => {
                  if((standardDirty||standardPending||editDirty)){setError('Keep or explicitly discard the original draft before changing directory connection.');return;}
                  if(canonical) update({connection:e.target.value,object:null});
                  setConnectionId(e.target.value);
                  setFieldPreview(null);
                  setPreview(null);
                  setEdit(null);
                  setMappings(
                    initial.connections.find((c) => c.id === e.target.value)?.mappings ?? [],
                  );
                }}
              >
                {initial.connections.map((c) => (
                  <option data-ui-appearance={appearance} key={c.id} value={c.id}>
                    {c.name} · {c.provider}
                  </option>
                ))}
              </Select>
            </Field>
            <p data-ui-appearance={appearance}>
              {connection.writeEnabled ? "Write-back enabled" : "Read-only"} ·{" "}
              {connection.inventory?.providerVersion ?? "Not tested"} ·{" "}
              {connection.inventory ? (
                <TimeText appearance={appearance} value={connection.inventory.observedAt} />
              ) : (
                "Read the inventory to begin"
              )}
            </p>
            {initial.canManage && (
              <Button appearance={appearance}
                type="submit"
                variant="secondary"

                disabled={busy || standardPending}
                onClick={() =>
                  void send("refresh", {
                    connectionId,
                    expectedVersion: connection.version,
                  })
                }
              >
                Read inventory
              </Button>
            )}
            {mode !== "settings" && initial.canManage && (
              <div data-ui-appearance={appearance} className={compactStyles.quickActions} aria-label="Directory actions">
                {primaryActions.map((kind) => (
                  <Button appearance={appearance}
                    variant="secondary"
                    key={kind}
                    type="button"
                    onClick={() => open({ kind })}
                  >
                    {labels[kind]}
                  </Button>
                ))}
                {moreActions.length > 0 && (
                  <details data-ui-appearance={appearance}
                    ref={actionMenu}
                    className={compactStyles.moreActions}
                    onKeyDown={(event) => {
                      if (event.key === "Escape") actionMenu.current?.removeAttribute("open");
                    }}
                  >
                    <summary data-ui-appearance={appearance}>More actions</summary>
                    <div data-ui-appearance={appearance} className={compactStyles.moreMenu}>
                      {moreActions.map((kind) => (
                        <Button appearance={appearance}
                          variant="plain"
                          key={kind}
                          type="button"
                          onClick={() => {
                            actionMenu.current?.removeAttribute("open");
                            open({ kind });
                          }}
                        >
                          {labels[kind]}
                        </Button>
                      ))}
                    </div>
                  </details>
                )}
              </div>
            )}
          </div>
          {section !== "work" && (mode === "settings" ? (
            <>
              <p data-ui-appearance={appearance}>
                Only fields supported by the current connector schema are available. Each mapping
                has one direction.
              </p>
              <div className={styles.stack}>{mappings.map((mapping, index) => <article key={index}><strong>{attributes.find(attribute => attribute.id === mapping.attributeId)?.label ?? mapping.attributeId}</strong><p>{mapping.field} · {mapping.direction === "read" ? "Directory → ERPX" : "ERPX → Directory"}</p></article>)}</div>
              <FormDisclosure appearance="standard" retainFormContents open={mappingsOpen} onToggle={event=>setMappingsOpen(event.currentTarget.open)} drawerTitle="Configure field mappings" error={error} dismissDisabled={(busy||standardPending)}><summary>Configure field mappings</summary>
              {mappings.map((m, i) => (
                <fieldset data-ui-appearance={appearance} key={i}>
                  <Field appearance={appearance}>
                    Person attribute
                    <Select appearance={appearance}
                      value={m.attributeId}
                      onChange={(e) =>
                        setMappings(
                          mappings.map((v, n) =>
                            n === i ? { ...v, attributeId: e.target.value } : v,
                          ),
                        )
                      }
                    >
                      <option data-ui-appearance={appearance} value="">Choose a field</option>
                      {attributes
                        .filter((a) => a.target === "person" && !a.archived)
                        .map((a) => (
                          <option data-ui-appearance={appearance} key={a.id} value={a.id}>
                            {a.label}
                          </option>
                        ))}
                    </Select>
                  </Field>
                  <Field appearance={appearance}>
                    Directory field
                    <Select appearance={appearance}
                      value={m.field}
                      onChange={(e) =>
                        setMappings(
                          mappings.map((v, n) => (n === i ? { ...v, field: e.target.value } : v)),
                        )
                      }
                    >
                      <option data-ui-appearance={appearance} value="">Choose a supported field</option>
                      {connection.inventory?.fields.map((f) => (
                        <option data-ui-appearance={appearance} key={f.name} value={f.name}>
                          {f.name}
                        </option>
                      ))}
                    </Select>
                  </Field>
                  <Field appearance={appearance}>
                    Direction
                    <Select appearance={appearance}
                      value={m.direction}
                      onChange={(e) =>
                        setMappings(
                          mappings.map((v, n) =>
                            n === i
                              ? {
                                  ...v,
                                  direction: e.target.value as "read" | "write",
                                }
                              : v,
                          ),
                        )
                      }
                    >
                      <option data-ui-appearance={appearance} value="read">Directory → ERPX</option>
                      <option data-ui-appearance={appearance} value="write">ERPX → Directory</option>
                    </Select>
                  </Field>
                  <Button appearance={appearance}
                    type="submit"
                    variant="secondary"

                    data-directory-draft-action={true}
                    onClick={() => setMappings(mappings.filter((_, n) => n !== i))}
                  >
                    Remove mapping
                  </Button>
                </fieldset>
              ))}
              {initial.canManage && (
                <>
                  <Button appearance={appearance}
                    type="submit"
                    variant="secondary"

                    data-directory-draft-action={true}
                    onClick={() =>
                      setMappings([...mappings, { attributeId: "", field: "", direction: "read" }])
                    }
                  >
                    Add field mapping
                  </Button>
                  <Button appearance={appearance}
                    type="submit"
                    variant="secondary"

                    disabled={busy || !connection.inventory}
                    onClick={() =>
                      void send("configure", {
                        connectionId,
                        expectedVersion: connection.version,
                        writeEnabled: connection.writeEnabled,
                        mappings,
                      })
                    }
                  >
                    Save mappings
                  </Button>
                  <Button appearance={appearance}
                    type="submit"
                    variant="primary"
                    className="primary"
                    disabled={busy || !connection.inventory}
                    onClick={() =>
                      void send("configure", {
                        connectionId,
                        expectedVersion: connection.version,
                        writeEnabled: !connection.writeEnabled,
                      })
                    }
                  >
                    {connection.writeEnabled ? "Return to read-only" : "Enable tested write-back"}
                  </Button>
                </>
              )}
              {recovery}{standardPending && retainedSurface==="mappings" && <Button appearance="standard" type="button" variant="secondary" disabled={busy} onClick={()=>{if(!busy)setMappingsOpen(false);}}>Close and retain request</Button>}</FormDisclosure>
              {(initial.canManage || retainedSurface==="fields") && (
                <FormDisclosure appearance="standard" retainFormContents open={fieldsOpen} onToggle={event=>setFieldsOpen(event.currentTarget.open)} drawerTitle="Synchronize fields" error={error} dismissDisabled={(busy||standardPending)}><summary>Synchronize fields for a verified person</summary><form data-ui-appearance={appearance}
                  className={styles.stack}
                  onChange={() => setFieldPreview(null)}
                  onSubmit={async (e) => {
                    e.preventDefault();
                    const form = new FormData(e.currentTarget),
                      input = {
                        connectionId,
                        personId: String(form.get("personId")),
                        direction: String(form.get("direction")),
                        reason: String(form.get("reason")),
                      };
                    const result = await send("fields/preview", input);
                    if (result && validMappedFieldPreview(result)) setFieldPreview({ ...result, input });
                  }}
                >
                  <h3 data-ui-appearance={appearance}>Synchronize fields for a verified person</h3>
                  <Field appearance={appearance}>
                    Person
                    <Select appearance={appearance} name="personId" required>
                      <option data-ui-appearance={appearance} value="">Choose a person</option>
                      {people.map((p) => (
                        <option data-ui-appearance={appearance} key={p.id} value={p.id}>
                          {p.name}
                        </option>
                      ))}
                    </Select>
                  </Field>
                  <Field appearance={appearance}>
                    Direction
                    <Select appearance={appearance} name="direction">
                      <option data-ui-appearance={appearance} value="read">Directory → ERPX</option>
                      <option data-ui-appearance={appearance} value="write">ERPX → Directory, with independent review</option>
                    </Select>
                  </Field>
                  <Field appearance={appearance}>
                    Reason
                    <TextInput appearance={appearance} name="reason" minLength={3} required />
                  </Field>
                  <Button appearance={appearance} clientManaged type="submit" variant="secondary" disabled={busy || standardPending || !initial.canManage}>
                    Preview mapped fields
                  </Button>
                </form>
              {fieldPreview && (
                <div data-ui-appearance={appearance} className={styles.preview}>
                  <h3 data-ui-appearance={appearance}>{fieldPreview.personName}</h3>
                  {fieldPreview.changes.map((change) => (
                    <p data-ui-appearance={appearance} key={change.label}>
                      {change.label}: {String(change.from ?? "Empty")} →{" "}
                      {String(change.to ?? "Empty")}
                    </p>
                  ))}
                  <Button appearance={appearance}
                    type="submit"
                    variant="primary"
                    className="primary"
                    disabled={busy || standardPending || !initial.canManage}
                    onClick={async () => {
                      const result = await send("fields/sync", {
                        ...fieldPreview.input,
                        previewToken: fieldPreview.previewToken,
                      });
                      if (result) {
                        setNotice(
                          fieldPreview.input.direction === "read"
                            ? "Mapped fields updated."
                            : "Directory field update submitted for independent review.",
                        );
                        setFieldPreview(null);
                      }
                    }}
                  >
                    Confirm reviewed field changes
                  </Button>
                </div>
              )}
                {recovery}{standardPending && retainedSurface==="fields" && <Button appearance="standard" type="button" variant="secondary" disabled={busy} onClick={()=>{if(!busy)setFieldsOpen(false);}}>Close and retain request</Button>}</FormDisclosure>
              )}

            </>
          ) : (
            <>
              <ObjectConsole
                key={connectionId}
                objects={objects}
                initialObjectId={initialObjectId}
                connectionId={connectionId}
                canManage={initial.canManage}
                detailSuppressed={!!edit}
                onEdit={open}
                onDrop={drop}
                search={search}
                onSearchChange={setSearch}
              />
            </>
          ))}
          {(section === undefined || section === "work") && <details open={section === "work"} data-ui-appearance={appearance}>
            <summary data-ui-appearance={appearance}>Directory work & verification</summary>
            <div data-ui-appearance={appearance} className={styles.stack}>
              {initial.jobs
                .filter((j) => j.connection_id === connectionId)
                .map((j) => (
                  <article data-ui-appearance={appearance} key={j.id}>
                    <strong data-ui-appearance={appearance}>{labels[j.definition.action.kind]}</strong> ·{" "}
                    {j.status.replaceAll("_", " ")}
                    <p data-ui-appearance={appearance}>{j.definition.reason}</p>
                    {j.last_error && <p data-ui-appearance={appearance} role="status">{j.last_error}</p>}
                    {initial.canApply &&
                      j.approval_mode === "direct" &&
                      (j.status === "retry" ||
                        (j.status === "blocked" && j.last_error === "FRESH_MFA_REQUIRED")) && (
                        <Button appearance={appearance}
                          type="submit"
                          variant="secondary"

                          disabled={busy || standardPending}
                          onClick={() =>
                            void send("retry", {
                              id: j.id,
                              expectedVersion: j.version,
                            })
                          }
                        >
                          Retry unfinished steps
                        </Button>
                      )}
                    {j.steps?.map((step) => (
                      <p data-ui-appearance={appearance} key={step.index}>
                        Step {step.index + 1}: {step.status}
                      </p>
                    ))}
                    {j.status === "completed" && (
                      <p data-ui-appearance={appearance}>
                        Directory read-back verified ·{" "}
                        {j.definition.action.kind.startsWith("gpo.")
                          ? "Endpoint confirmation is tracked separately."
                          : ""}
                      </p>
                    )}
                    {j.review_id && j.status === "pending_review" && (
                      <Link href={base + "/technology?view=settings&section=reviews"}>
                        Open independent review
                      </Link>
                    )}
                    {initial.canManage &&
                      ["pending_review", "queued", "blocked"].includes(j.status) && (
                        <Button appearance={appearance}
                          type="submit"
                          variant="secondary"

                          disabled={busy || standardPending}
                          onClick={() =>
                            void send("cancel", {
                              id: j.id,
                              expectedVersion: j.version,
                            })
                          }
                        >
                          Cancel before execution
                        </Button>
                      )}
                  </article>
                ))}
            </div>
          </details>}
        </>
      )}
      {recovery}
      {retainedSurface && (standardPending || retainedSurface==="edit" && editorClosed && !!edit || retainedSurface==="mappings" && !mappingsOpen || retainedSurface==="fields" && !fieldsOpen) && <Button appearance="standard" type="button" variant="secondary" disabled={busy} onClick={() => {if (!busy) {if(retainedSurface==="edit")setEditorClosed(false);else if(retainedSurface==="mappings")setMappingsOpen(true);else setFieldsOpen(true);}}}>Reopen retained directory form</Button>}
      {error && <p data-ui-appearance={appearance} role="alert">{error}</p>}
      {notice && <p data-ui-appearance={appearance} role="status">{notice}</p>}
      <DraftFormDrawer appearance={appearance}
        dirty={editDirty || standardDirty}
        onDiscard={() => {if(standardCommands.current?.pending())return;setEdit(null);setEditDirty(false);setStandardDirty(false);}}
        dismissDisabled={busy || standardPending}
        retainFormContents={(editDirty||standardDirty||standardPending)}
        open={!!edit && !editorClosed}
        onClose={() => {if(!busy && !standardCommands.current?.pending())setEdit(null);}}
        title={edit ? labels[edit.kind] : "Directory change"}
      >
        {recovery}
        {standardPending && <Button appearance="standard" type="button" variant="secondary" disabled={busy} onClick={()=>{if(!busy)setEditorClosed(true);}}>Close and retain request</Button>}
        {edit&&editConnection!==connectionId&&<p role="status">Return to the original directory connection to continue this retained draft.</p>}
        {edit && connection && (
          <form data-ui-appearance={appearance}
            className={styles.stack}
            onSubmit={review}
            onChangeCapture={() => setEditDirty(true)}
            onChange={() => {
              setPreview(null);
              setRequest(null);
            }}
          >
            {!edit.action &&
              !["account.create", "group.create", "ou.create"].includes(edit.kind) && (
                <Field appearance={appearance}>
                  {edit.kind === "gpo.publish"
                    ? "Existing GPO, or create a new one"
                    : "Directory object"}
                  <Select appearance={appearance}
                    name="objectId"
                    defaultValue={edit.objectId ?? ""}
                    onChange={(e) => {
                      open({ ...edit, objectId: e.target.value || undefined });
                    }}
                    required={edit.kind !== "gpo.publish"}
                  >
                    <option data-ui-appearance={appearance} value="">
                      {edit.kind === "gpo.publish"
                        ? "Create a new managed GPO"
                        : "Choose an object"}
                    </option>
                    {options(
                      objects.filter((o) =>
                        ["ou.update", "gpo.order", "gpo.inheritance"].includes(edit.kind)
                          ? o.kind === "ou"
                          : edit.kind.startsWith("gpo.")
                            ? o.kind === "gpo"
                            : edit.kind === "group.members"
                              ? o.kind === "group"
                              : edit.kind === "object.move"
                                ? ["account", "computer", "group", "ou"].includes(o.kind)
                                : o.kind === "account",
                      ),
                    )}
                  </Select>
                </Field>
              )}
            {["account.create", "group.create", "ou.create", "object.move", "gpo.link"].includes(
              edit.kind,
            ) && (
              <Field appearance={appearance}>
                Target OU / container
                <Select appearance={appearance} name="parentId" required defaultValue={edit.parentId ?? ""}>
                  <option data-ui-appearance={appearance} value="">Choose an OU</option>
                  {options(ous)}
                </Select>
              </Field>
            )}
            {edit.kind === "account.create" && (
              <>
                <Field appearance={appearance}>
                  Username
                  <TextInput appearance={appearance} name="username" required pattern="[a-zA-Z][a-zA-Z0-9._-]{1,63}" />
                </Field>
                <Field appearance={appearance}>
                  First name
                  <TextInput appearance={appearance} name="firstName" required />
                </Field>
                <Field appearance={appearance}>
                  Last name
                  <TextInput appearance={appearance} name="lastName" required />
                </Field>
                <p data-ui-appearance={appearance}>
                  The account is created disabled. Linking a person and activating access require
                  independent identity and lifecycle review.
                </p>
              </>
            )}
            {["group.create", "ou.create"].includes(edit.kind) && (
              <Field appearance={appearance}>
                Name
                <TextInput appearance={appearance} name="name" required />
              </Field>
            )}
            {edit.kind === "account.update" && (
              <>
                <Field appearance={appearance}>
                  Supported field
                  <Select appearance={appearance} name="field">
                    {connection.inventory?.fields
                      .filter((f) => f.write)
                      .map((f) => (
                        <option data-ui-appearance={appearance} key={f.name}>{f.name}</option>
                      ))}
                  </Select>
                </Field>
                <Field appearance={appearance}>
                  New value
                  <TextInput appearance={appearance} name="value" maxLength={2000} />
                </Field>
              </>
            )}
            {edit.kind === "account.disable" && (
              <>
                <Field appearance={appearance}>
                  Related departure (optional)
                  <Select appearance={appearance} name="caseId">
                    <option data-ui-appearance={appearance} value="">Independent IT action</option>
                    {initial.cases
                      .filter(
                        (c) =>
                          ["offboarding", "suspend"].includes(c.kind) &&
                          c.accountIds.includes(edit.objectId ?? ""),
                      )
                      .map((c) => (
                        <option data-ui-appearance={appearance} key={c.id} value={c.id}>
                          {c.name} · {c.kind} · <TimeText appearance={appearance} value={c.effectiveAt} />
                        </option>
                      ))}
                  </Select>
                </Field>
                <p data-ui-appearance={appearance}>
                  ERPX departure cutoffs continue to apply independently of this directory
                  operation. This disables the whole directory account.
                </p>
              </>
            )}
            {edit.kind === "group.members" && (
              <>
                <Field appearance={appearance}>
                  Members
                  <Select appearance={appearance}
                    name="memberIds"
                    multiple
                    required
                    defaultValue={edit.memberId ? [edit.memberId] : []}
                  >
                    {options(
                      objects.filter((o) => ["account", "group", "computer"].includes(o.kind)),
                    )}
                  </Select>
                </Field>
                <Field appearance={appearance}>
                  Action
                  <Select appearance={appearance} name="action">
                    <option data-ui-appearance={appearance} value="add">Add membership</option>
                    <option data-ui-appearance={appearance} value="remove">Remove membership</option>
                  </Select>
                </Field>
              </>
            )}
            {edit.action && (
              <p data-ui-appearance={appearance}>
                New link order:{" "}
                {edit.action.kind === "gpo.order"
                  ? edit.action.gpoIds
                      .map((id) => objects.find((o) => o.id === id)?.name ?? id)
                      .join(" → ")
                  : edit.action.kind}
              </p>
            )}
            {edit.kind === "ou.update" && (
              <Field appearance={appearance}>
                Description
                <TextInput appearance={appearance}
                  name="description"
                  maxLength={500}
                  defaultValue={String(
                    objects.find((o) => o.id === edit.objectId)?.properties.description ?? "",
                  )}
                />
              </Field>
            )}
            {edit.kind === "gpo.inheritance" && (
              <Field appearance={appearance} orientation="inline">
                <TextInput appearance={appearance}
                  type="checkbox"
                  name="blocked"
                  defaultChecked={
                    !!objects.find((o) => o.id === edit.objectId)?.properties.blockInheritance
                  }
                />{" "}
                Block inheritance (enforced links still apply)
              </Field>
            )}
            {edit.kind === "gpo.filter" && (
              <Field appearance={appearance}>
                Security filter groups
                <Select appearance={appearance}
                  name="groupIds"
                  multiple
                  required
                  defaultValue={
                    (objects.find((o) => o.id === edit.objectId)?.properties
                      .securityFilterGroupIds ?? []) as string[]
                  }
                >
                  {options(groups)}
                </Select>
                <span data-ui-appearance={appearance}>Read permissions for authenticated computers are preserved.</span>
              </Field>
            )}
            {edit.kind === "gpo.link" && (
              <>
                <Field appearance={appearance} orientation="inline">
                  <TextInput appearance={appearance}
                    type="checkbox"
                    name="enabled"
                    defaultChecked={existingLink?.enabled ?? true}
                  />{" "}
                  Link enabled
                </Field>
                <Field appearance={appearance} orientation="inline">
                  <TextInput appearance={appearance}
                    type="checkbox"
                    name="enforced"
                    defaultChecked={existingLink?.enforced ?? false}
                  />{" "}
                  Enforced
                </Field>
                <Field appearance={appearance}>
                  Order (1 is highest precedence)
                  <TextInput appearance={appearance}
                    type="number"
                    min={1}
                    max={1000}
                    name="order"
                    defaultValue={
                      existingLink ? existingLinks.indexOf(existingLink) + 1 : undefined
                    }
                  />
                </Field>
                <Field appearance={appearance} orientation="inline">
                  <TextInput appearance={appearance} type="checkbox" name="remove" /> Remove this link
                </Field>
                <p data-ui-appearance={appearance}>
                  Security filtering is shared by every link of this GPO. Edit it in GPO properties.
                </p>
              </>
            )}
            {edit.kind === "account.enable" && (
              <Field appearance={appearance}>
                Reviewed onboarding / transfer
                <Select appearance={appearance} name="caseId" required>
                  <option data-ui-appearance={appearance} value="">Choose the employee start or transfer</option>
                  {initial.cases
                    .filter(
                      (c) =>
                        ["onboarding", "transfer"].includes(c.kind) &&
                        c.accountIds.includes(edit.objectId ?? ""),
                    )
                    .map((c) => (
                      <option data-ui-appearance={appearance} key={c.id} value={c.id}>
                        {c.name} · {c.kind} · <TimeText appearance={appearance} value={c.effectiveAt} />
                      </option>
                    ))}
                </Select>
                <span data-ui-appearance={appearance}>
                  Activation runs only after personal review and the effective start time.
                </span>
              </Field>
            )}
            {edit.kind === "gpo.publish" && (
              <WindowsPolicyFields appearance={appearance}
                value={policy}
                settingsOnly={!!edit.objectId}
                onChange={(v) => {
                  setPolicy(v);
                  setPreview(null);
                  setRequest(null);
                }}
                objects={objects}
                definitions={
                  objects.find((o) => o.id === edit.objectId)?.properties.definitions as
                    Partial<Record<"user" | "computer", WindowsPolicyDefinition>> | undefined
                }
              />
            )}
            <Field appearance={appearance}>
              Reason
              <TextArea appearance={appearance} name="reason" required minLength={3} maxLength={1000} />
            </Field>
            <Button appearance={appearance} clientManaged type="submit" variant="secondary" disabled={busy || standardPending}>
              Preview impact
            </Button>
            {preview && request && (
              <div data-ui-appearance={appearance} className={styles.preview}>
                <h3 data-ui-appearance={appearance}>Review the directory impact</h3>
                <p data-ui-appearance={appearance}>Objects: {preview.objects.join(", ") || "New directory object"}</p>
                <p data-ui-appearance={appearance}>Positions: {preview.positions.join(", ") || "None"}</p>
                <p data-ui-appearance={appearance}>People: {preview.people.join(", ") || "None linked"}</p>
                <p data-ui-appearance={appearance}>Computers: {preview.computers.join(", ") || "None in the observed inventory"}</p>
                <p data-ui-appearance={appearance}>Companies: {preview.companies.map((c) => c.name).join(", ")}</p>
                {preview.warnings.map((w) => (
                  <p data-ui-appearance={appearance} key={w}>{w}</p>
                ))}
                <Button appearance={appearance}
                  variant="primary"
                  type="button"
                  className="primary"
                  disabled={busy || !connection.writeEnabled || standardPending}
                  onClick={async () => {
                    if(editConnection!==connectionId){setError('Return to the original directory connection to continue this draft.');return;}
                    const confirmed = {
                      ...request,
                      affectedCompanyIds: preview.companies.map((c) => c.id),
                    };
                    const latest = await send("preview", confirmed);
                    if (!validDirectoryPreview(latest)) return;
                    // Acknowledgment changes the request token; never silently accept a changed impact.
                    if (
                      JSON.stringify({ ...latest, token: "" }) !==
                      JSON.stringify({ ...preview, token: "" })
                    ) {
                      setRequest(confirmed);
                      setPreview(latest);
                      return;
                    }
                    const r = await send(initial.canApply ? "apply" : "submit", {
                      ...confirmed,
                      previewToken: latest.token,
                    });
                    if (r) {
                      setEdit(null);
                      setNotice(
                        initial.canApply
                          ? "Apply queued. Follow each step and verified read-back below."
                          : "Submitted for independent review.",
                      );
                    }
                  }}
                >
                  {initial.canApply ? "Apply changes" : "Submit for independent review"}
                </Button>
              </div>
            )}
            {error && <p data-ui-appearance={appearance} role="alert">{error}</p>}
          </form>
        )}
      </DraftFormDrawer>
    </section>
  );
  return <DirectoryDraftBoundary dirty={standardDirty} pending={standardPending||busy}>{content}</DirectoryDraftBoundary>;
}

function WindowsPolicyFields({ appearance = "standard",
  value: p,
  onChange: set,
  objects,
  definitions,
  settingsOnly = false,
}: ({
  settingsOnly?: boolean;
  definitions?: Partial<Record<"user" | "computer", WindowsPolicyDefinition>>;
  value: WindowsPolicyDefinition;
  onChange: (v: WindowsPolicyDefinition) => void;
  objects: NonNullable<DirectoryAutomationData["connections"][number]["inventory"]>["objects"];
}) & { appearance?: "standard" }) {
  const choice = (kind: "ou" | "group") =>
    objects
      .filter((o) => o.kind === kind)
      .map((o) => (
        <option data-ui-appearance={appearance} key={o.id} value={o.id}>
          {o.name}
        </option>
      ));
  return (
    <>
      {settingsOnly && (
        <p data-ui-appearance={appearance}>
          Editing settings preserves this GPO’s links and security filtering. Use the Properties
          actions to change them. Shared targets are included in the preview.
        </p>
      )}
      <Field appearance={appearance}>
        Policy name
        <TextInput appearance={appearance} value={p.name} onChange={(e) => set({ ...p, name: e.target.value })} />
      </Field>
      <Field appearance={appearance}>
        Applies to
        <Select appearance={appearance}
          value={p.scope}
          onChange={(e) =>
            set(
              definitions?.[e.target.value as typeof p.scope] ?? {
                ...p,
                scope: e.target.value as typeof p.scope,
                usb: "unchanged",
                lockSeconds: null,
                wallpaper: null,
                drives: [],
                printers: [],
              },
            )
          }
        >
          <option data-ui-appearance={appearance} value="user">Users</option>
          <option data-ui-appearance={appearance} value="computer">Computers</option>
        </Select>
      </Field>
      {!settingsOnly && (
        <>
          <Field appearance={appearance}>
            Linked OU
            <Select appearance={appearance}
              value={p.targetOuId}
              onChange={(e) => set({ ...p, targetOuId: e.target.value })}
            >
              <option data-ui-appearance={appearance} value="unselected">Choose an OU</option>
              {choice("ou")}
            </Select>
          </Field>
          <Field appearance={appearance}>
            Security filter group
            <Select appearance={appearance}
              value={p.filterGroupId}
              onChange={(e) => set({ ...p, filterGroupId: e.target.value })}
            >
              <option data-ui-appearance={appearance} value="unselected">Choose a group</option>
              {choice("group")}
            </Select>
          </Field>
        </>
      )}
      {p.scope === "computer" && (
        <Field appearance={appearance}>
          Computer OU or group
          <Select appearance={appearance}
            value={p.computerTargetId ?? ""}
            onChange={(e) => set({ ...p, computerTargetId: e.target.value || null })}
          >
            <option data-ui-appearance={appearance} value="">Choose an explicit computer target</option>
            {choice("ou")}
            {choice("group")}
          </Select>
        </Field>
      )}
      <fieldset data-ui-appearance={appearance}>
        <legend data-ui-appearance={appearance}>Windows administrative policies</legend>
        <Field appearance={appearance}>
          USB / removable storage
          <Select appearance={appearance}
            value={p.usb}
            onChange={(e) => set({ ...p, usb: e.target.value as typeof p.usb })}
          >
            <option data-ui-appearance={appearance} value="unchanged">Leave unchanged</option>
            <option data-ui-appearance={appearance} value="deny">Deny all removable storage access</option>
            <option data-ui-appearance={appearance} value="allow">Allow removable storage access</option>
          </Select>
        </Field>
        <Field appearance={appearance}>
          Lock after inactivity (seconds)
          <TextInput appearance={appearance}
            type="number"
            min={60}
            max={86400}
            value={p.lockSeconds ?? ""}
            onChange={(e) =>
              set({
                ...p,
                lockSeconds: e.target.value ? Number(e.target.value) : null,
              })
            }
          />
        </Field>
        {p.scope === "user" && (
          <>
            <Field appearance={appearance}>
              Desktop wallpaper path
              <TextInput appearance={appearance}
                value={p.wallpaper ?? ""}
                placeholder="\\server\share\wallpaper.jpg"
                onChange={(e) => set({ ...p, wallpaper: e.target.value || null })}
              />
            </Field>
            <Field appearance={appearance}>
              Wallpaper layout
              <Select appearance={appearance}
                value={p.wallpaperStyle}
                onChange={(e) =>
                  set({
                    ...p,
                    wallpaperStyle: e.target.value as typeof p.wallpaperStyle,
                  })
                }
              >
                <option data-ui-appearance={appearance} value="0">Center</option>
                <option data-ui-appearance={appearance} value="2">Stretch</option>
                <option data-ui-appearance={appearance} value="6">Fit</option>
                <option data-ui-appearance={appearance} value="10">Fill</option>
              </Select>
            </Field>
          </>
        )}
      </fieldset>
      {p.scope === "user" && (
        <fieldset data-ui-appearance={appearance}>
          <legend data-ui-appearance={appearance}>Drive & Printer preferences</legend>
          {p.drives.map((d, i) => (
            <fieldset data-ui-appearance={appearance} key={d.id}>
              <legend data-ui-appearance={appearance}>Drive {i + 1}</legend>
              <Field appearance={appearance}>
                Letter
                <TextInput appearance={appearance}
                  maxLength={1}
                  value={d.letter}
                  onChange={(e) =>
                    set({
                      ...p,
                      drives: p.drives.map((v, n) =>
                        n === i ? { ...v, letter: e.target.value.toUpperCase() } : v,
                      ),
                    })
                  }
                />
              </Field>
              <Field appearance={appearance}>
                Share path
                <TextInput appearance={appearance}
                  value={d.path}
                  onChange={(e) =>
                    set({
                      ...p,
                      drives: p.drives.map((v, n) =>
                        n === i ? { ...v, path: e.target.value } : v,
                      ),
                    })
                  }
                />
              </Field>
              <Field appearance={appearance}>
                Label
                <TextInput appearance={appearance}
                  value={d.label}
                  onChange={(e) =>
                    set({
                      ...p,
                      drives: p.drives.map((v, n) =>
                        n === i ? { ...v, label: e.target.value } : v,
                      ),
                    })
                  }
                />
              </Field>
              <Field appearance={appearance} orientation="inline">
                <TextInput appearance={appearance}
                  type="checkbox"
                  checked={d.remove}
                  onChange={(e) =>
                    set({
                      ...p,
                      drives: p.drives.map((v, n) =>
                        n === i ? { ...v, remove: e.target.checked } : v,
                      ),
                    })
                  }
                />
                Remove this mapping from targeted users
              </Field>
              <Button appearance={appearance}
                variant="secondary"
                type="button"

                onClick={() => set({ ...p, drives: p.drives.filter((_, n) => n !== i) })}
              >
                Remove from recipe
              </Button>
            </fieldset>
          ))}
          <Button appearance={appearance}
            variant="secondary"
            type="button"

            onClick={() =>
              set({
                ...p,
                drives: [
                  ...p.drives,
                  {
                    id: crypto.randomUUID(),
                    letter: "S",
                    path: "",
                    label: "Shared drive",
                    remove: false,
                  },
                ],
              })
            }
          >
            Add Drive preference
          </Button>
          {p.printers.map((d, i) => (
            <fieldset data-ui-appearance={appearance} key={d.id}>
              <legend data-ui-appearance={appearance}>Printer {i + 1}</legend>
              <Field appearance={appearance}>
                Printer share path
                <TextInput appearance={appearance}
                  value={d.path}
                  onChange={(e) =>
                    set({
                      ...p,
                      printers: p.printers.map((v, n) =>
                        n === i ? { ...v, path: e.target.value } : v,
                      ),
                    })
                  }
                />
              </Field>
              <Field appearance={appearance} orientation="inline">
                <TextInput appearance={appearance}
                  type="checkbox"
                  checked={d.isDefault}
                  onChange={(e) =>
                    set({
                      ...p,
                      printers: p.printers.map((v, n) =>
                        n === i ? { ...v, isDefault: e.target.checked } : v,
                      ),
                    })
                  }
                />
                Default printer
              </Field>
              <Field appearance={appearance} orientation="inline">
                <TextInput appearance={appearance}
                  type="checkbox"
                  checked={d.remove}
                  onChange={(e) =>
                    set({
                      ...p,
                      printers: p.printers.map((v, n) =>
                        n === i ? { ...v, remove: e.target.checked } : v,
                      ),
                    })
                  }
                />
                Remove this printer connection
              </Field>
              <Button appearance={appearance}
                variant="secondary"
                type="button"

                onClick={() => set({ ...p, printers: p.printers.filter((_, n) => n !== i) })}
              >
                Remove from recipe
              </Button>
            </fieldset>
          ))}
          <Button appearance={appearance}
            variant="secondary"
            type="button"

            onClick={() =>
              set({
                ...p,
                printers: [
                  ...p.printers,
                  {
                    id: crypto.randomUUID(),
                    path: "",
                    isDefault: false,
                    remove: false,
                  },
                ],
              })
            }
          >
            Add Printer preference
          </Button>
        </fieldset>
      )}
      <p data-ui-appearance={appearance}>
        Publishing verifies directory configuration. Endpoint results require a separate Windows
        verification run.
      </p>
    </>
  );
}
