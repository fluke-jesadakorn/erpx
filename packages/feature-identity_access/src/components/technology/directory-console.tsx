"use client";
import { Button, TextInput } from "@erpx/ui/components/foundation";

import { WorkspaceItemActions } from "@erpx/ui/components/workspace-item-actions";
import { WorkspaceCollection, WorkspaceDetail } from "@erpx/ui/components/workspace";
import { useSearchParams } from "next/navigation";
import { useState } from "react";
import {
  FolderTree,
  Folder,
  Users,
  Monitor,
  UserRound,
  FileCog,
  ArrowUp,
  ArrowDown,
  ArrowLeft,
  GripVertical,
  ChevronRight,
} from "lucide-react";
import type { DirectoryAction, DirectoryObject } from "@erpx/feature-identity_access/lib/directory-operations";
import styles from "@erpx/feature-identity_access/components/technology/directory-console.module.css";
const icons = {
  ou: Folder,
  group: Users,
  computer: Monitor,
  account: UserRound,
  gpo: FileCog,
};
type Edit = {
  kind: DirectoryAction["kind"];
  objectId?: string;
  parentId?: string;
  memberId?: string;
  action?: DirectoryAction;
};
export function DirectoryConsole({
  objects,
  connectionId,
  canManage,
  onEdit,
  onDrop,
  search,
  onSearchChange,
  initialObjectId,
  detailSuppressed = false,
}: {
  objects: DirectoryObject[];
  initialObjectId?: string;
  detailSuppressed?: boolean;
  connectionId: string;
  canManage: boolean;
  onEdit: (e: Edit) => void;
  onDrop: (raw: string, target: string, kind: string) => void;
  search: string;
  onSearchChange: (value: string) => void;
}) {
  const query = useSearchParams();
  const [folder, setFolder] = useState("all"),
    [pickerOpen, setPickerOpen] = useState(false),
    [selected, setSelected] = useState<string | null>(initialObjectId ?? null);
  const selection = query.get("view") === "directory" ? query.get("object") ?? selected : selected;
  const current = objects.find((o) => o.id === selection),
    ous = objects.filter((o) => o.kind === "ou");
  const folderName =
    folder === "all"
      ? "All objects"
      : folder === "groups"
        ? "Groups"
        : folder === "gpos"
          ? "Group Policy Objects"
          : (ous.find((o) => o.id === folder)?.name ?? "All objects");
  const listing = objects.filter(
    (o) =>
      (folder === "all" ||
        (folder === "gpos"
          ? o.kind === "gpo"
          : folder === "groups"
            ? o.kind === "group"
            : o.parentId === folder)) &&
      (o.name + " " + o.kind + " " + o.dn).toLowerCase().includes(search.toLowerCase()),
  );
  const depth = (o: DirectoryObject) => {
    let n = 0,
      p = o.parentId;
    const seen = new Set([o.id]);
    while (p && !seen.has(p) && n < 12) {
      seen.add(p);
      n++;
      p = objects.find((x) => x.id === p)?.parentId ?? null;
    }
    return n;
  };
  const drag = (o: DirectoryObject) => ({
    draggable: canManage,
    onDragStart: (e: React.DragEvent) =>
      e.dataTransfer.setData(
        "application/erpx-directory-object",
        JSON.stringify({ connectionId, id: o.id }),
      ),
    onDragOver: (e: React.DragEvent) => {
      if (canManage && ["group", "ou"].includes(o.kind)) e.preventDefault();
    },
    onDrop: (e: React.DragEvent) => {
      if (!canManage) return;
      e.preventDefault();
      onDrop(e.dataTransfer.getData("application/erpx-directory-object"), o.id, o.kind);
    },
  });
  const links = (current?.properties.gpoLinks ?? []) as {
    gpoId: string;
    enabled: boolean;
    enforced: boolean;
  }[];
  function order(from: number, to: number) {
    if (!current || to < 0 || to >= links.length) return;
    const ids = links.map((l) => l.gpoId);
    const [id] = ids.splice(from, 1);
    ids.splice(to, 0, id);
    onEdit({
      kind: "gpo.order",
      objectId: current.id,
      action: { kind: "gpo.order", objectId: current.id, gpoIds: ids },
    });
  }
  function chooseObject(id: string | null) {
    setSelected(id);
    const url = new URL(window.location.href);
    if(url.pathname.endsWith("/technology") && url.searchParams.get("view") === "directory") {
      if(id) url.searchParams.set("object", id); else url.searchParams.delete("object");
      url.searchParams.set("connection", connectionId);
      window.history.pushState(null, "", url);
    }
  }
  function chooseFolder(id: string) {
    setFolder(id);
    setPickerOpen(false);
    chooseObject(null);
  }
  return (
    <WorkspaceCollection detailOpen={Boolean(current) && !detailSuppressed}>
      <section className={styles.objectList} aria-label="Directory objects">
        <div className={styles.listToolbar}>
          <div>
            <strong>Objects</strong>
            <span>{listing.length} shown</span>
          </div>
          <TextInput
            type="search"
            className={styles.search}
            aria-label="Search directory objects"
            placeholder="Search objects"
            value={search}
            onChange={(event) => onSearchChange(event.target.value)}
          />
          <Button
            variant="plain"
            type="button"
            className={styles.locationButton}
            aria-expanded={pickerOpen}
            aria-controls="directory-location-picker"
            onClick={() => setPickerOpen(!pickerOpen)}
          >
            <FolderTree size={16} /> {folderName}
          </Button>
        </div>
        {pickerOpen && (
          <nav
            id="directory-location-picker"
            className={styles.tree}
            aria-label="Directory OU tree"
          >
            <p>Choose a location or drag an object onto an OU or group.</p>
            <Button
              variant="plain"
              type="button"
              aria-current={folder === "all"}
              onClick={() => chooseFolder("all")}
            >
              All objects
            </Button>
            {ous.map((o) => (
              <Button
                variant="plain"
                type="button"
                key={o.id}
                {...drag(o)}
                style={{ paddingInlineStart: 12 + depth(o) * 15 }}
                aria-current={folder === o.id}
                onClick={() => chooseFolder(o.id)}
              >
                <Folder size={15} />
                {o.name}
              </Button>
            ))}
            <Button
              variant="plain"
              type="button"
              aria-current={folder === "groups"}
              onClick={() => chooseFolder("groups")}
            >
              <Users size={15} />
              Groups
            </Button>
            <Button
              variant="plain"
              type="button"
              aria-current={folder === "gpos"}
              onClick={() => chooseFolder("gpos")}
            >
              <FileCog size={15} />
              Group Policy Objects
            </Button>
          </nav>
        )}
        <div className={styles.rows} role="list">
          {listing.map((o) => {
            const Icon = icons[o.kind];
            const {draggable,onDragStart,...targetProps} = drag(o);
            const move = () => onEdit({kind:"object.move",objectId:o.id});
            return (
              <div
                key={o.id}
                className={styles.row}
                role="listitem"
                {...targetProps}
                data-selected={selection === o.id}
              >
                <Button
                  variant="plain"
                  type="button"
                  aria-current={selection === o.id}
                  onClick={() => chooseObject(o.id)}
                  onDoubleClick={() => o.kind === "ou" && chooseFolder(o.id)}
                >
                  <Icon size={18} aria-hidden="true" />
                  <span>
                    <strong>{o.name}</strong>
                    <small>
                      {o.kind} ·{" "}
                      {objects.find((p) => p.id === o.parentId)?.name ?? "Directory root"}
                    </small>
                  </span>
                  <ChevronRight size={16} aria-hidden="true"/>
                </Button>
                {canManage && o.kind !== "gpo" && <WorkspaceItemActions name={o.name} onMove={move} grip={<Button appearance="standard" variant="plain" type="button" aria-label={`Drag ${o.name}`} title="Drag to move; press Enter to choose a destination" draggable={draggable} onDragStart={onDragStart} onClick={move}><GripVertical size={16}/></Button>}/>}
              </div>
            );
          })}
          {!listing.length && (
            <p className={styles.empty}>
              {search.trim()
                ? "No objects match this search."
                : "No objects in this location. Choose another location or read the inventory."}
            </p>
          )}
        </div>
      </section>
      <WorkspaceDetail open={!!current && !detailSuppressed} title={current?.name ?? "Object properties"} onClose={() => chooseObject(null)} className={styles.properties}>
        {current ? (
          <>
            <Button
              variant="plain"
              type="button"
              className={styles.backButton}
              onClick={() => chooseObject(null)}
            >
              <ArrowLeft size={16} /> Back to objects
            </Button>

            <span className={styles.tag}>{current.kind}</span>
            <dl>
              <dt>Directory location</dt>
              <dd>{current.dn}</dd>
              <dt>Permanent identity</dt>
              <dd>{current.id}</dd>
            </dl>
            {current.kind === "group" && (
              <>
                <h4>Direct members</h4>
                <ul>
                  {current.memberIds.map((id) => (
                    <li key={id}>
                      <Button type="submit" variant="plain" onClick={() => chooseObject(id)}>
                        {objects.find((o) => o.id === id)?.name ?? id}
                      </Button>
                    </li>
                  ))}
                </ul>
              </>
            )}
            <h4>Member of</h4>
            <ul>
              {objects
                .filter((o) => o.kind === "group" && o.memberIds.includes(current.id))
                .map((o) => (
                  <li key={o.id}>
                    <Button type="submit" variant="plain" onClick={() => chooseObject(o.id)}>
                      {o.name}
                    </Button>
                  </li>
                ))}
            </ul>
            {current.kind === "ou" && (
              <>
                <h4>Linked Group Policy Objects</h4>
                <p>
                  Order 1 has highest precedence. Inheritance:{" "}
                  {current.properties.blockInheritance ? "Blocked" : "Enabled"}
                </p>
                <ol>
                  {links.map((l, i) => (
                    <li
                      key={l.gpoId}
                      onDragOver={(e) => e.preventDefault()}
                      onDrop={(e) => {
                        e.preventDefault();
                        const raw = e.dataTransfer.getData("application/erpx-gpo-link");
                        if (raw !== "") order(Number(raw), i);
                      }}
                    >
                      <Button type="submit" variant="plain" onClick={() => chooseObject(l.gpoId)}>
                        {objects.find((o) => o.id === l.gpoId)?.name ?? l.gpoId}
                      </Button>
                      <small>
                        {l.enabled ? "Enabled" : "Disabled"}
                        {l.enforced ? " · Enforced" : ""}
                      </small>
                      {canManage && (
                        <div className={styles.linkActions}>
                          <Button appearance="standard" type="button" variant="plain" aria-label={`Drag GPO link ${i + 1}`} draggable onDragStart={event => {event.stopPropagation();event.dataTransfer.setData("application/erpx-gpo-link",String(i));}}><GripVertical size={16}/></Button>
                          <Button
                            type="submit"
                            variant="plain"
                            aria-label={`Move GPO link ${i + 1} up`}
                            disabled={i === 0}
                            onClick={() => order(i, i - 1)}
                          >
                            <ArrowUp size={14} />
                          </Button>
                          <Button
                            type="submit"
                            variant="plain"
                            aria-label={`Move GPO link ${i + 1} down`}
                            disabled={i === links.length - 1}
                            onClick={() => order(i, i + 1)}
                          >
                            <ArrowDown size={14} />
                          </Button>
                          <Button
                            type="submit"
                            variant="plain"
                            onClick={() =>
                              onEdit({
                                kind: "gpo.link",
                                objectId: l.gpoId,
                                parentId: current.id,
                              })
                            }
                          >
                            Properties
                          </Button>
                        </div>
                      )}
                    </li>
                  ))}
                </ol>
              </>
            )}
            {canManage && (
              <div className={styles.propertyActions}>
                {["account", "computer", "group", "ou"].includes(current.kind) && (
                  <Button
                    type="submit"
                    variant="secondary"

                    onClick={() => onEdit({ kind: "object.move", objectId: current.id })}
                  >
                    Move to OU
                  </Button>
                )}
                {current.kind === "group" && (
                  <Button
                    type="submit"
                    variant="secondary"

                    onClick={() => onEdit({ kind: "group.members", objectId: current.id })}
                  >
                    Change members
                  </Button>
                )}
                {current.kind === "ou" && (
                  <>
                    <Button
                      type="submit"
                      variant="secondary"

                      onClick={() => onEdit({ kind: "ou.update", objectId: current.id })}
                    >
                      Edit OU
                    </Button>
                    <Button
                      type="submit"
                      variant="secondary"

                      onClick={() =>
                        onEdit({
                          kind: "gpo.inheritance",
                          objectId: current.id,
                        })
                      }
                    >
                      Set inheritance
                    </Button>
                  </>
                )}
                {current.kind === "gpo" && (
                  <>
                    <Button
                      type="submit"
                      variant="secondary"

                      onClick={() => onEdit({ kind: "gpo.publish", objectId: current.id })}
                    >
                      Edit settings
                    </Button>
                    <Button
                      type="submit"
                      variant="secondary"

                      onClick={() => onEdit({ kind: "gpo.link", objectId: current.id })}
                    >
                      Link to OU
                    </Button>
                    <Button
                      type="submit"
                      variant="secondary"

                      onClick={() => onEdit({ kind: "gpo.filter", objectId: current.id })}
                    >
                      Security filtering
                    </Button>
                  </>
                )}
                {current.kind === "account" && (
                  <>
                    <Button
                      type="submit"
                      variant="secondary"

                      onClick={() => onEdit({ kind: "account.update", objectId: current.id })}
                    >
                      Edit account
                    </Button>
                    <Button
                      type="submit"
                      variant="secondary"

                      onClick={() => onEdit({ kind: "account.enable", objectId: current.id })}
                    >
                      Enable account
                    </Button>
                    <Button
                      type="submit"
                      variant="secondary"

                      onClick={() =>
                        onEdit({
                          kind: "account.disable",
                          objectId: current.id,
                        })
                      }
                    >
                      Disable account
                    </Button>
                  </>
                )}
              </div>
            )}
          </>
        ) : (
          <div className={styles.empty}>
            <FileCog size={28} />
            <h3>Properties</h3>
            <p>Select an account, group, OU, computer or GPO.</p>
            <p>Every change opens a preview before Apply.</p>
          </div>
        )}
      </WorkspaceDetail>
    </WorkspaceCollection>
  );
}
