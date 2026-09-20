"use client";
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
} from "lucide-react";
import type {
  DirectoryAction,
  DirectoryObject,
} from "@/lib/directory-operations";
import styles from "./technology.module.css";
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
}: {
  objects: DirectoryObject[];
  connectionId: string;
  canManage: boolean;
  onEdit: (e: Edit) => void;
  onDrop: (raw: string, target: string, kind: string) => void;
  search: string;
}) {
  const [folder, setFolder] = useState("all"),
    [selected, setSelected] = useState<string | null>(null);
  const current = objects.find((o) => o.id === selected),
    ous = objects.filter((o) => o.kind === "ou");
  const listing = objects.filter(
    (o) =>
      (folder === "all" ||
        (folder === "gpos"
          ? o.kind === "gpo"
          : folder === "groups"
            ? o.kind === "group"
            : o.parentId === folder)) &&
      (o.name + " " + o.kind + " " + o.dn)
        .toLowerCase()
        .includes(search.toLowerCase()),
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
      onDrop(
        e.dataTransfer.getData("application/erpx-directory-object"),
        o.id,
        o.kind,
      );
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
  return (
    <div className={styles.console}>
      <nav className={styles.tree} aria-label="Directory OU tree">
        <h3>
          <FolderTree size={17} /> Directory
        </h3>
        <button
          aria-current={folder === "all"}
          onClick={() => setFolder("all")}
        >
          All objects
        </button>
        {ous.map((o) => (
          <button
            key={o.id}
            {...drag(o)}
            style={{ paddingInlineStart: 12 + depth(o) * 15 }}
            aria-current={folder === o.id}
            onClick={() => {
              setFolder(o.id);
              setSelected(o.id);
            }}
          >
            <Folder size={15} />
            {o.name}
          </button>
        ))}
        <hr />
        <button
          aria-current={folder === "groups"}
          onClick={() => setFolder("groups")}
        >
          <Users size={15} /> Group membership
        </button>
        <button
          aria-current={folder === "gpos"}
          onClick={() => setFolder("gpos")}
        >
          <FileCog size={15} /> Group Policy Objects
        </button>
        <p>
          OU placement and group membership are separate. Groups can belong to
          several groups.
        </p>
      </nav>
      <div className={styles.objectList}>
        <table>
          <thead>
            <tr>
              <th>Name</th>
              <th>Type</th>
              <th>Location</th>
            </tr>
          </thead>
          <tbody>
            {listing.map((o) => {
              const Icon = icons[o.kind];
              return (
                <tr key={o.id} {...drag(o)} data-selected={selected === o.id}>
                  <td>
                    <button
                      onClick={() => setSelected(o.id)}
                      onDoubleClick={() => o.kind === "ou" && setFolder(o.id)}
                    >
                      <Icon size={16} />
                      {o.name}
                    </button>
                  </td>
                  <td>{o.kind}</td>
                  <td>
                    {objects.find((p) => p.id === o.parentId)?.name ??
                      "Directory root"}
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
        {!listing.length && (
          <p className={styles.empty}>
            No objects in this view. Read the directory inventory to refresh.
          </p>
        )}
        <div className={styles.status}>
          {listing.length} objects · Select an object to inspect its properties
        </div>
      </div>
      <aside className={styles.properties} aria-label="Object properties">
        {current ? (
          <>
            <h3>{current.name}</h3>
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
                      <button onClick={() => setSelected(id)}>
                        {objects.find((o) => o.id === id)?.name ?? id}
                      </button>
                    </li>
                  ))}
                </ul>
              </>
            )}
            <h4>Member of</h4>
            <ul>
              {objects
                .filter(
                  (o) => o.kind === "group" && o.memberIds.includes(current.id),
                )
                .map((o) => (
                  <li key={o.id}>
                    <button onClick={() => setSelected(o.id)}>{o.name}</button>
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
                      draggable={canManage}
                      onDragStart={(e) => {
                        e.stopPropagation();
                        e.dataTransfer.setData(
                          "application/erpx-gpo-link",
                          String(i),
                        );
                      }}
                      onDragOver={(e) => e.preventDefault()}
                      onDrop={(e) => {
                        e.preventDefault();
                        const raw = e.dataTransfer.getData(
                          "application/erpx-gpo-link",
                        );
                        if (raw !== "") order(Number(raw), i);
                      }}
                    >
                      <button onClick={() => setSelected(l.gpoId)}>
                        {objects.find((o) => o.id === l.gpoId)?.name ?? l.gpoId}
                      </button>
                      <small>
                        {l.enabled ? "Enabled" : "Disabled"}
                        {l.enforced ? " · Enforced" : ""}
                      </small>
                      {canManage && (
                        <div className={styles.linkActions}>
                          <button
                            aria-label={`Move GPO link ${i + 1} up`}
                            disabled={i === 0}
                            onClick={() => order(i, i - 1)}
                          >
                            <ArrowUp size={14} />
                          </button>
                          <button
                            aria-label={`Move GPO link ${i + 1} down`}
                            disabled={i === links.length - 1}
                            onClick={() => order(i, i + 1)}
                          >
                            <ArrowDown size={14} />
                          </button>
                          <button
                            onClick={() =>
                              onEdit({
                                kind: "gpo.link",
                                objectId: l.gpoId,
                                parentId: current.id,
                              })
                            }
                          >
                            Properties
                          </button>
                        </div>
                      )}
                    </li>
                  ))}
                </ol>
              </>
            )}
            {canManage && (
              <div className={styles.propertyActions}>
                {["account", "computer", "group", "ou"].includes(
                  current.kind,
                ) && (
                  <button
                    className="button secondary"
                    onClick={() =>
                      onEdit({ kind: "object.move", objectId: current.id })
                    }
                  >
                    Move to OU
                  </button>
                )}
                {current.kind === "group" && (
                  <button
                    className="button secondary"
                    onClick={() =>
                      onEdit({ kind: "group.members", objectId: current.id })
                    }
                  >
                    Change members
                  </button>
                )}
                {current.kind === "ou" && (
                  <>
                    <button
                      className="button secondary"
                      onClick={() =>
                        onEdit({ kind: "ou.update", objectId: current.id })
                      }
                    >
                      Edit OU
                    </button>
                    <button
                      className="button secondary"
                      onClick={() =>
                        onEdit({
                          kind: "gpo.inheritance",
                          objectId: current.id,
                        })
                      }
                    >
                      Set inheritance
                    </button>
                  </>
                )}
                {current.kind === "gpo" && (
                  <>
                    <button
                      className="button secondary"
                      onClick={() =>
                        onEdit({ kind: "gpo.publish", objectId: current.id })
                      }
                    >
                      Edit settings
                    </button>
                    <button
                      className="button secondary"
                      onClick={() =>
                        onEdit({ kind: "gpo.link", objectId: current.id })
                      }
                    >
                      Link to OU
                    </button>
                    <button
                      className="button secondary"
                      onClick={() =>
                        onEdit({ kind: "gpo.filter", objectId: current.id })
                      }
                    >
                      Security filtering
                    </button>
                  </>
                )}
                {current.kind === "account" && (
                  <>
                    <button
                      className="button secondary"
                      onClick={() =>
                        onEdit({ kind: "account.update", objectId: current.id })
                      }
                    >
                      Edit account
                    </button>
                    <button
                      className="button secondary"
                      onClick={() =>
                        onEdit({ kind: "account.enable", objectId: current.id })
                      }
                    >
                      Enable account
                    </button>
                    <button
                      className="button secondary"
                      onClick={() =>
                        onEdit({
                          kind: "account.disable",
                          objectId: current.id,
                        })
                      }
                    >
                      Disable account
                    </button>
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
      </aside>
    </div>
  );
}
