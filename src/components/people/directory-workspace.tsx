"use client";
import { useRef, useState, type FormEvent } from "react";
import { useRouter } from "next/navigation";
import Link from "next/link";
import { Dialog } from "@/components/ui/dialog";
import { workFetch } from "@/lib/work-fetch";
import {
  directoryChangeInput,
  windowsPolicyDefinition,
  type DirectoryAction,
  type DirectoryAutomationData,
  type DirectoryPreview,
  type DirectoryChangeInput,
  type WindowsPolicyDefinition,
} from "@/lib/directory-operations";
import type { AttributeDefinition } from "@/lib/user-management";
import styles from "./lifecycle.module.css";
import { DirectoryConsole } from "@/components/technology/directory-console";
import compactStyles from "@/components/technology/directory-workspace.module.css";

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
export function DirectoryWorkspace({
  base,
  initial,
  mode = "inventory",
  attributes = [],
  people = [],
  initialConnectionId,
  initialObjectId,
}: {
  base: string;
  initialConnectionId?: string;
  initialObjectId?: string;
  initial: DirectoryAutomationData;
  mode?: "inventory" | "gpo" | "settings" | "groups";
  attributes?: AttributeDefinition[];
  people?: { id: string; name: string }[];
}) {
  const router = useRouter(),
    receipt = useRef<{ body: string; key: string } | null>(null);
  const actionMenu = useRef<HTMLDetailsElement>(null);
  const [connectionId, setConnectionId] = useState(
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
  const connection = initial.connections.find((c) => c.id === connectionId),
    objects = connection?.inventory?.objects ?? [],
    ous = objects.filter((o) => o.kind === "ou"),
    groups = objects.filter((o) => o.kind === "group");
  const existingLinks = (objects.find((o) => o.id === edit?.parentId)
    ?.properties.gpoLinks ?? []) as {
    gpoId: string;
    enabled: boolean;
    enforced: boolean;
  }[];
  const existingLink = existingLinks.find((l) => l.gpoId === edit?.objectId);
  const [search, setSearch] = useState("");
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
  async function send(path: string, input: unknown) {
    setBusy(true);
    setError("");
    const body = JSON.stringify(input),
      fingerprint = path + body;
    if (receipt.current?.body !== fingerprint)
      receipt.current = { body: fingerprint, key: crypto.randomUUID() };
    try {
      const r = await workFetch(
        "/api/v1" + base + "/people/directory-operations/" + path,
        {
          method: "POST",
          headers: {
            "Content-Type": "application/json",
            "Idempotency-Key": receipt.current.key,
          },
          body,
        },
      );
      const result = await r.json();
      if (!r.ok)
        throw Error(result.error?.message ?? "The directory operation failed.");
      receipt.current = null;
      if (path !== "preview") router.refresh();
      return result.data;
    } catch (e) {
      setError(e instanceof Error ? e.message : "Unable to continue.");
    } finally {
      setBusy(false);
    }
  }
  function open(value: NonNullable<typeof edit>) {
    if (value.kind === "gpo.publish") {
      const saved = value.objectId
        ? objects.find((o) => o.id === value.objectId)?.properties.definition
        : null;
      setPolicy(
        windowsPolicyDefinition.parse(
          saved ?? {
            name:
              objects.find((o) => o.id === value.objectId)?.name ??
              "Windows policy",
            scope: "user",
            targetOuId: "unselected",
            filterGroupId: "unselected",
          },
        ),
      );
    }
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
      if (
        targetKind === "group" &&
        ["account", "computer", "group"].includes(source.kind)
      )
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
      else
        throw Error(
          "This drop is not supported. Choose an action from the object.",
        );
    } catch (e) {
      setError(e instanceof Error ? e.message : "Choose a supported drop.");
    }
  }
  async function review(e: FormEvent<HTMLFormElement>) {
    e.preventDefault();
    if (!edit || !connection) return;
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
    if (result) {
      setRequest(parsed.data);
      setPreview(result);
    }
  }
  const options = (items: typeof objects) =>
    items.map((o) => (
      <option key={o.id} value={o.id}>
        {o.name} · {o.kind}
      </option>
    ));
  const actionKinds: DirectoryAction["kind"][] =
    mode === "gpo"
      ? ["gpo.publish"]
      : mode === "groups"
        ? ["group.create"]
        : ["account.create", "group.create", "ou.create", "gpo.publish", "object.move", "group.members"];
  const primaryActions = mode === "inventory" ? actionKinds.slice(0, 2) : actionKinds;
  const moreActions = mode === "inventory" ? actionKinds.slice(2) : [];
  return (
    <section className={"card " + styles.stack + (mode === "settings" ? "" : " " + compactStyles.workspace)}>
      {mode !== "inventory" && <h2>
        {mode === "gpo"
          ? "Windows policies"
          : mode === "settings"
            ? "Directory write-back & field mapping"
            : mode === "groups"
              ? "Directory groups"
              : "Directory accounts, computers & OUs"}
      </h2>}
      {!connection ? (
        <p>
          No active UCS or AD connection.{" "}
          <Link href={base + "/people/settings?section=connections"}>
            Configure a connection
          </Link>
          .
        </p>
      ) : (
        <>
          <div className={mode === "settings" ? styles.stack : compactStyles.workspaceHeader}>
          <label>
            {mode === "settings" ? "Directory connection" : "Connection"}
            <select
              value={connectionId}
              onChange={(e) => {
                setConnectionId(e.target.value);
                setFieldPreview(null);
                setPreview(null);
                setEdit(null);
                setMappings(
                  initial.connections.find((c) => c.id === e.target.value)
                    ?.mappings ?? [],
                );
              }}
            >
              {initial.connections.map((c) => (
                <option key={c.id} value={c.id}>
                  {c.name} · {c.provider}
                </option>
              ))}
            </select>
          </label>
          <p>
            {connection.writeEnabled ? "Write-back enabled" : "Read-only"} ·{" "}
            {connection.inventory?.providerVersion ?? "Not tested"} ·{" "}
            {connection.inventory
              ? new Date(connection.inventory.observedAt).toLocaleString()
              : "Read the inventory to begin"}
          </p>
          {initial.canManage && (
            <button
              className="button secondary"
              disabled={busy}
              onClick={() =>
                void send("refresh", {
                  connectionId,
                  expectedVersion: connection.version,
                })
              }
            >
              Read inventory
            </button>
          )}
          {mode !== "settings" && initial.canManage && (
            <div className={compactStyles.quickActions} aria-label="Directory actions">
              {primaryActions.map((kind) => (
                <button key={kind} type="button" className="button secondary" onClick={() => open({ kind })}>
                  {labels[kind]}
                </button>
              ))}
              {moreActions.length > 0 && <details ref={actionMenu} className={compactStyles.moreActions} onKeyDown={(event) => {
                if (event.key === "Escape") actionMenu.current?.removeAttribute("open");
              }}>
                <summary>More actions</summary>
                <div className={compactStyles.moreMenu}>
                  {moreActions.map((kind) => <button key={kind} type="button" onClick={() => {
                    actionMenu.current?.removeAttribute("open");
                    open({ kind });
                  }}>{labels[kind]}</button>)}
                </div>
              </details>}
            </div>
          )}
          </div>
          {mode === "settings" ? (
            <>
              <p>
                Only fields supported by the current connector schema are
                available. Each mapping has one direction.
              </p>
              {mappings.map((m, i) => (
                <fieldset key={i}>
                  <label>
                    Person attribute
                    <select
                      value={m.attributeId}
                      onChange={(e) =>
                        setMappings(
                          mappings.map((v, n) =>
                            n === i ? { ...v, attributeId: e.target.value } : v,
                          ),
                        )
                      }
                    >
                      <option value="">Choose a field</option>
                      {attributes
                        .filter((a) => a.target === "person" && !a.archived)
                        .map((a) => (
                          <option key={a.id} value={a.id}>
                            {a.label}
                          </option>
                        ))}
                    </select>
                  </label>
                  <label>
                    Directory field
                    <select
                      value={m.field}
                      onChange={(e) =>
                        setMappings(
                          mappings.map((v, n) =>
                            n === i ? { ...v, field: e.target.value } : v,
                          ),
                        )
                      }
                    >
                      <option value="">Choose a supported field</option>
                      {connection.inventory?.fields.map((f) => (
                        <option key={f.name} value={f.name}>
                          {f.name}
                        </option>
                      ))}
                    </select>
                  </label>
                  <label>
                    Direction
                    <select
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
                      <option value="read">Directory → ERPX</option>
                      <option value="write">ERPX → Directory</option>
                    </select>
                  </label>
                  <button
                    className="button secondary"
                    onClick={() =>
                      setMappings(mappings.filter((_, n) => n !== i))
                    }
                  >
                    Remove mapping
                  </button>
                </fieldset>
              ))}
              {initial.canManage && (
                <>
                  <button
                    className="button secondary"
                    onClick={() =>
                      setMappings([
                        ...mappings,
                        { attributeId: "", field: "", direction: "read" },
                      ])
                    }
                  >
                    Add field mapping
                  </button>
                  <button
                    className="button secondary"
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
                  </button>
                  <button
                    className="button primary"
                    disabled={busy || !connection.inventory}
                    onClick={() =>
                      void send("configure", {
                        connectionId,
                        expectedVersion: connection.version,
                        writeEnabled: !connection.writeEnabled,
                      })
                    }
                  >
                    {connection.writeEnabled
                      ? "Return to read-only"
                      : "Enable tested write-back"}
                  </button>
                </>
              )}
              {initial.canManage && (
                <form
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
                    if (result) setFieldPreview({ ...result, input });
                  }}
                >
                  <h3>Synchronize fields for a verified person</h3>
                  <label>
                    Person
                    <select name="personId" required>
                      <option value="">Choose a person</option>
                      {people.map((p) => (
                        <option key={p.id} value={p.id}>
                          {p.name}
                        </option>
                      ))}
                    </select>
                  </label>
                  <label>
                    Direction
                    <select name="direction">
                      <option value="read">Directory → ERPX</option>
                      <option value="write">
                        ERPX → Directory, with independent review
                      </option>
                    </select>
                  </label>
                  <label>
                    Reason
                    <input name="reason" minLength={3} required />
                  </label>
                  <button className="button secondary" disabled={busy}>
                    Preview mapped fields
                  </button>
                </form>
              )}
              {fieldPreview && (
                <div className={styles.preview}>
                  <h3>{fieldPreview.personName}</h3>
                  {fieldPreview.changes.map((change) => (
                    <p key={change.label}>
                      {change.label}: {String(change.from ?? "Empty")} →{" "}
                      {String(change.to ?? "Empty")}
                    </p>
                  ))}
                  <button
                    className="button primary"
                    disabled={busy}
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
                  </button>
                </div>
              )}
            </>
          ) : (
            <>
              <DirectoryConsole
                key={connectionId}
                objects={objects}
                initialObjectId={initialObjectId}
                connectionId={connectionId}
                canManage={initial.canManage}
                onEdit={open}
                onDrop={drop}
                search={search}
                onSearchChange={setSearch}
              />
            </>
          )}
          <details>
            <summary>Directory work & verification</summary>
            <div className={styles.stack}>
              {initial.jobs
                .filter((j) => j.connection_id === connectionId)
                .map((j) => (
                  <article key={j.id}>
                    <strong>{labels[j.definition.action.kind]}</strong> ·{" "}
                    {j.status.replaceAll("_", " ")}
                    <p>{j.definition.reason}</p>
                    {j.last_error && <p role="status">{j.last_error}</p>}
                    {initial.canApply &&
                      j.approval_mode === "direct" &&
                      (j.status === "retry" ||
                        (j.status === "blocked" &&
                          j.last_error === "FRESH_MFA_REQUIRED")) && (
                        <button
                          className="button secondary"
                          disabled={busy}
                          onClick={() =>
                            void send("retry", {
                              id: j.id,
                              expectedVersion: j.version,
                            })
                          }
                        >
                          Retry unfinished steps
                        </button>
                      )}
                    {j.steps?.map((step) => (
                      <p key={step.index}>
                        Step {step.index + 1}: {step.status}
                      </p>
                    ))}
                    {j.status === "completed" && (
                      <p>
                        Directory read-back verified ·{" "}
                        {j.definition.action.kind.startsWith("gpo.")
                          ? "Endpoint confirmation is tracked separately."
                          : ""}
                      </p>
                    )}
                    {j.review_id && j.status === "pending_review" && (
                      <Link href={base + "/people/settings?section=reviews"}>
                        Open independent review
                      </Link>
                    )}
                    {initial.canManage &&
                      ["pending_review", "queued", "blocked"].includes(
                        j.status,
                      ) && (
                        <button
                          className="button secondary"
                          disabled={busy}
                          onClick={() =>
                            void send("cancel", {
                              id: j.id,
                              expectedVersion: j.version,
                            })
                          }
                        >
                          Cancel before execution
                        </button>
                      )}
                  </article>
                ))}
            </div>
          </details>
        </>
      )}
      {error && <p role="alert">{error}</p>}
      {notice && <p role="status">{notice}</p>}
      <Dialog
        open={!!edit}
        onClose={() => setEdit(null)}
        title={edit ? labels[edit.kind] : "Directory change"}
        presentation="responsive-panel"
      >
        {edit && connection && (
          <form
            className={styles.stack}
            onSubmit={review}
            onChange={() => {
              setPreview(null);
              setRequest(null);
            }}
          >
            {!edit.action &&
              !["account.create", "group.create", "ou.create"].includes(
                edit.kind,
              ) && (
                <label>
                  {edit.kind === "gpo.publish"
                    ? "Existing GPO, or create a new one"
                    : "Directory object"}
                  <select
                    name="objectId"
                    defaultValue={edit.objectId ?? ""}
                    onChange={(e) => {
                      open({ ...edit, objectId: e.target.value || undefined });
                    }}
                    required={edit.kind !== "gpo.publish"}
                  >
                    <option value="">
                      {edit.kind === "gpo.publish"
                        ? "Create a new managed GPO"
                        : "Choose an object"}
                    </option>
                    {options(
                      objects.filter((o) =>
                        ["ou.update", "gpo.order", "gpo.inheritance"].includes(
                          edit.kind,
                        )
                          ? o.kind === "ou"
                          : edit.kind.startsWith("gpo.")
                            ? o.kind === "gpo"
                            : edit.kind === "group.members"
                              ? o.kind === "group"
                              : edit.kind === "object.move"
                                ? [
                                    "account",
                                    "computer",
                                    "group",
                                    "ou",
                                  ].includes(o.kind)
                                : o.kind === "account",
                      ),
                    )}
                  </select>
                </label>
              )}
            {[
              "account.create",
              "group.create",
              "ou.create",
              "object.move",
              "gpo.link",
            ].includes(edit.kind) && (
              <label>
                Target OU / container
                <select
                  name="parentId"
                  required
                  defaultValue={edit.parentId ?? ""}
                >
                  <option value="">Choose an OU</option>
                  {options(ous)}
                </select>
              </label>
            )}
            {edit.kind === "account.create" && (
              <>
                <label>
                  Username
                  <input
                    name="username"
                    required
                    pattern="[a-zA-Z][a-zA-Z0-9._-]{1,63}"
                  />
                </label>
                <label>
                  First name
                  <input name="firstName" required />
                </label>
                <label>
                  Last name
                  <input name="lastName" required />
                </label>
                <p>
                  The account is created disabled. Linking a person and
                  activating access require independent identity and lifecycle
                  review.
                </p>
              </>
            )}
            {["group.create", "ou.create"].includes(edit.kind) && (
              <label>
                Name
                <input name="name" required />
              </label>
            )}
            {edit.kind === "account.update" && (
              <>
                <label>
                  Supported field
                  <select name="field">
                    {connection.inventory?.fields
                      .filter((f) => f.write)
                      .map((f) => (
                        <option key={f.name}>{f.name}</option>
                      ))}
                  </select>
                </label>
                <label>
                  New value
                  <input name="value" maxLength={2000} />
                </label>
              </>
            )}
            {edit.kind === "account.disable" && (
              <>
                <label>
                  Related departure (optional)
                  <select name="caseId">
                    <option value="">Independent IT action</option>
                    {initial.cases
                      .filter(
                        (c) =>
                          ["offboarding", "suspend"].includes(c.kind) &&
                          c.accountIds.includes(edit.objectId ?? ""),
                      )
                      .map((c) => (
                        <option key={c.id} value={c.id}>
                          {c.name} · {c.kind} ·{" "}
                          {new Date(c.effectiveAt).toLocaleString()}
                        </option>
                      ))}
                  </select>
                </label>
                <p>
                  ERPX departure cutoffs continue to apply independently of this
                  directory operation. This disables the whole directory
                  account.
                </p>
              </>
            )}
            {edit.kind === "group.members" && (
              <>
                <label>
                  Members
                  <select
                    name="memberIds"
                    multiple
                    required
                    defaultValue={edit.memberId ? [edit.memberId] : []}
                  >
                    {options(
                      objects.filter((o) =>
                        ["account", "group", "computer"].includes(o.kind),
                      ),
                    )}
                  </select>
                </label>
                <label>
                  Action
                  <select name="action">
                    <option value="add">Add membership</option>
                    <option value="remove">Remove membership</option>
                  </select>
                </label>
              </>
            )}
            {edit.action && (
              <p>
                New link order:{" "}
                {edit.action.kind === "gpo.order"
                  ? edit.action.gpoIds
                      .map((id) => objects.find((o) => o.id === id)?.name ?? id)
                      .join(" → ")
                  : edit.action.kind}
              </p>
            )}
            {edit.kind === "ou.update" && (
              <label>
                Description
                <input
                  name="description"
                  maxLength={500}
                  defaultValue={String(
                    objects.find((o) => o.id === edit.objectId)?.properties
                      .description ?? "",
                  )}
                />
              </label>
            )}
            {edit.kind === "gpo.inheritance" && (
              <label>
                <input
                  type="checkbox"
                  name="blocked"
                  defaultChecked={
                    !!objects.find((o) => o.id === edit.objectId)?.properties
                      .blockInheritance
                  }
                />{" "}
                Block inheritance (enforced links still apply)
              </label>
            )}
            {edit.kind === "gpo.filter" && (
              <label>
                Security filter groups
                <select
                  name="groupIds"
                  multiple
                  required
                  defaultValue={
                    (objects.find((o) => o.id === edit.objectId)?.properties
                      .securityFilterGroupIds ?? []) as string[]
                  }
                >
                  {options(groups)}
                </select>
                <span>
                  Read permissions for authenticated computers are preserved.
                </span>
              </label>
            )}
            {edit.kind === "gpo.link" && (
              <>
                <label>
                  <input
                    type="checkbox"
                    name="enabled"
                    defaultChecked={existingLink?.enabled ?? true}
                  />{" "}
                  Link enabled
                </label>
                <label>
                  <input
                    type="checkbox"
                    name="enforced"
                    defaultChecked={existingLink?.enforced ?? false}
                  />{" "}
                  Enforced
                </label>
                <label>
                  Order (1 is highest precedence)
                  <input
                    type="number"
                    min={1}
                    max={1000}
                    name="order"
                    defaultValue={
                      existingLink
                        ? existingLinks.indexOf(existingLink) + 1
                        : undefined
                    }
                  />
                </label>
                <label>
                  <input type="checkbox" name="remove" /> Remove this link
                </label>
                <p>
                  Security filtering is shared by every link of this GPO. Edit
                  it in GPO properties.
                </p>
              </>
            )}
            {edit.kind === "account.enable" && (
              <label>
                Reviewed onboarding / transfer
                <select name="caseId" required>
                  <option value="">
                    Choose the employee start or transfer
                  </option>
                  {initial.cases
                    .filter(
                      (c) =>
                        ["onboarding", "transfer"].includes(c.kind) &&
                        c.accountIds.includes(edit.objectId ?? ""),
                    )
                    .map((c) => (
                      <option key={c.id} value={c.id}>
                        {c.name} · {c.kind} ·{" "}
                        {new Date(c.effectiveAt).toLocaleString()}
                      </option>
                    ))}
                </select>
                <span>
                  Activation runs only after personal review and the effective
                  start time.
                </span>
              </label>
            )}
            {edit.kind === "gpo.publish" && (
              <WindowsPolicyFields
                value={policy}
                settingsOnly={!!edit.objectId}
                onChange={(v) => {
                  setPolicy(v);
                  setPreview(null);
                  setRequest(null);
                }}
                objects={objects}
                definitions={
                  objects.find((o) => o.id === edit.objectId)?.properties
                    .definitions as
                    | Partial<
                        Record<"user" | "computer", WindowsPolicyDefinition>
                      >
                    | undefined
                }
              />
            )}
            <label>
              Reason
              <textarea name="reason" required minLength={3} maxLength={1000} />
            </label>
            <button className="button secondary" disabled={busy}>
              Preview impact
            </button>
            {preview && request && (
              <div className={styles.preview}>
                <h3>Review the directory impact</h3>
                <p>
                  Objects:{" "}
                  {preview.objects.join(", ") || "New directory object"}
                </p>
                <p>Positions: {preview.positions.join(", ") || "None"}</p>
                <p>People: {preview.people.join(", ") || "None linked"}</p>
                <p>
                  Computers:{" "}
                  {preview.computers.join(", ") ||
                    "None in the observed inventory"}
                </p>
                <p>
                  Companies: {preview.companies.map((c) => c.name).join(", ")}
                </p>
                {preview.warnings.map((w) => (
                  <p key={w}>{w}</p>
                ))}
                <button
                  type="button"
                  className="button primary"
                  disabled={busy || !connection.writeEnabled}
                  onClick={async () => {
                    const confirmed = {
                      ...request,
                      affectedCompanyIds: preview.companies.map((c) => c.id),
                    };
                    const latest = await send("preview", confirmed);
                    if (!latest) return;
                    // Acknowledgment changes the request token; never silently accept a changed impact.
                    if (
                      JSON.stringify({ ...latest, token: "" }) !==
                      JSON.stringify({ ...preview, token: "" })
                    ) {
                      setRequest(confirmed);
                      setPreview(latest);
                      return;
                    }
                    const r = await send(
                      initial.canApply ? "apply" : "submit",
                      {
                        ...confirmed,
                        previewToken: latest.token,
                      },
                    );
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
                  {initial.canApply
                    ? "Apply changes"
                    : "Submit for independent review"}
                </button>
              </div>
            )}
            {error && <p role="alert">{error}</p>}
          </form>
        )}
      </Dialog>
    </section>
  );
}

function WindowsPolicyFields({
  value: p,
  onChange: set,
  objects,
  definitions,
  settingsOnly = false,
}: {
  settingsOnly?: boolean;
  definitions?: Partial<Record<"user" | "computer", WindowsPolicyDefinition>>;
  value: WindowsPolicyDefinition;
  onChange: (v: WindowsPolicyDefinition) => void;
  objects: NonNullable<
    DirectoryAutomationData["connections"][number]["inventory"]
  >["objects"];
}) {
  const choice = (kind: "ou" | "group") =>
    objects
      .filter((o) => o.kind === kind)
      .map((o) => (
        <option key={o.id} value={o.id}>
          {o.name}
        </option>
      ));
  return (
    <>
      {settingsOnly && (
        <p>
          Editing settings preserves this GPO’s links and security filtering.
          Use the Properties actions to change them. Shared targets are included
          in the preview.
        </p>
      )}
      <label>
        Policy name
        <input
          value={p.name}
          onChange={(e) => set({ ...p, name: e.target.value })}
        />
      </label>
      <label>
        Applies to
        <select
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
          <option value="user">Users</option>
          <option value="computer">Computers</option>
        </select>
      </label>
      {!settingsOnly && (
        <>
          <label>
            Linked OU
            <select
              value={p.targetOuId}
              onChange={(e) => set({ ...p, targetOuId: e.target.value })}
            >
              <option value="unselected">Choose an OU</option>
              {choice("ou")}
            </select>
          </label>
          <label>
            Security filter group
            <select
              value={p.filterGroupId}
              onChange={(e) => set({ ...p, filterGroupId: e.target.value })}
            >
              <option value="unselected">Choose a group</option>
              {choice("group")}
            </select>
          </label>
        </>
      )}
      {p.scope === "computer" && (
        <label>
          Computer OU or group
          <select
            value={p.computerTargetId ?? ""}
            onChange={(e) =>
              set({ ...p, computerTargetId: e.target.value || null })
            }
          >
            <option value="">Choose an explicit computer target</option>
            {choice("ou")}
            {choice("group")}
          </select>
        </label>
      )}
      <fieldset>
        <legend>Windows administrative policies</legend>
        <label>
          USB / removable storage
          <select
            value={p.usb}
            onChange={(e) => set({ ...p, usb: e.target.value as typeof p.usb })}
          >
            <option value="unchanged">Leave unchanged</option>
            <option value="deny">Deny all removable storage access</option>
            <option value="allow">Allow removable storage access</option>
          </select>
        </label>
        <label>
          Lock after inactivity (seconds)
          <input
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
        </label>
        {p.scope === "user" && (
          <>
            <label>
              Desktop wallpaper path
              <input
                value={p.wallpaper ?? ""}
                placeholder="\\server\share\wallpaper.jpg"
                onChange={(e) =>
                  set({ ...p, wallpaper: e.target.value || null })
                }
              />
            </label>
            <label>
              Wallpaper layout
              <select
                value={p.wallpaperStyle}
                onChange={(e) =>
                  set({
                    ...p,
                    wallpaperStyle: e.target.value as typeof p.wallpaperStyle,
                  })
                }
              >
                <option value="0">Center</option>
                <option value="2">Stretch</option>
                <option value="6">Fit</option>
                <option value="10">Fill</option>
              </select>
            </label>
          </>
        )}
      </fieldset>
      {p.scope === "user" && (
        <fieldset>
          <legend>Drive & Printer preferences</legend>
          {p.drives.map((d, i) => (
            <fieldset key={d.id}>
              <legend>Drive {i + 1}</legend>
              <label>
                Letter
                <input
                  maxLength={1}
                  value={d.letter}
                  onChange={(e) =>
                    set({
                      ...p,
                      drives: p.drives.map((v, n) =>
                        n === i
                          ? { ...v, letter: e.target.value.toUpperCase() }
                          : v,
                      ),
                    })
                  }
                />
              </label>
              <label>
                Share path
                <input
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
              </label>
              <label>
                Label
                <input
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
              </label>
              <label>
                <input
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
              </label>
              <button
                type="button"
                className="button secondary"
                onClick={() =>
                  set({ ...p, drives: p.drives.filter((_, n) => n !== i) })
                }
              >
                Remove from recipe
              </button>
            </fieldset>
          ))}
          <button
            type="button"
            className="button secondary"
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
          </button>
          {p.printers.map((d, i) => (
            <fieldset key={d.id}>
              <legend>Printer {i + 1}</legend>
              <label>
                Printer share path
                <input
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
              </label>
              <label>
                <input
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
              </label>
              <label>
                <input
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
              </label>
              <button
                type="button"
                className="button secondary"
                onClick={() =>
                  set({ ...p, printers: p.printers.filter((_, n) => n !== i) })
                }
              >
                Remove from recipe
              </button>
            </fieldset>
          ))}
          <button
            type="button"
            className="button secondary"
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
          </button>
        </fieldset>
      )}
      <p>
        Publishing verifies directory configuration. Endpoint results require a
        separate Windows verification run.
      </p>
    </>
  );
}
