"use client";
import { TimeText } from "@/components/time/time-preference-context";

import { useState } from "react";
import { ArrowLeft } from "lucide-react";
import Link from "next/link";
import { Dialog } from "@/components/ui/dialog";
import { workFetch } from "@/lib/work-fetch";
import styles from "./position-gpo-workspace.module.css";

type ObjectRef = { id: string; name: string; kind: string };
type Connection = { id: string; name: string; inventory: { objects: ObjectRef[] } | null };
type Position = { id: string; title: string; holder_count: number; mapping_version: number | null; mapped: boolean; mapping_status: "pending" | "directory_applied" | "blocked" | null };
type Policy = { position_id: string; version: number; revision: number; mapped: boolean; connection_id: string | null; gpo_id: string | null; user_filter_group_id: string | null; computer_target_ou_id: string | null; directory_config_status: "pending" | "directory_applied" | "blocked"; directory_config_error: string | null };
type Job = { id: string; position_id: string; legal_name: string; desired_action: string; status: string; version: number; last_error: string | null; directory_observed_at: number | null; endpoint_observed_at: number | null };
type Data = { positions: Position[]; policies: Policy[]; jobs: Job[] };
type Preview = {
  token: string;
  version: number;
  revision: number;
  holders: { engagement_id: string; name: string; account_id: string | null; accountStatus: "linked" | "awaiting_account_linkage" }[];
  changes: { membersToAdd: string[]; membersToRemoveFromPreviousGroup: string[] };
  directory: { gpo: { id: string; version: string }; observedAt: number };
  sharedMappings: { position_id: string }[];
};
type UnmapPreview = { token: string; title: string; revision: number; holders: { person_id: string; legal_name: string; account_ids: string[] }[]; sharedMappings: { position_id: string }[]; current: { gpoId: string; userFilterGroupId: string; computerTargetOuId: string } };

async function action(url: string, body: unknown) {
  const response = await workFetch(url, {
    method: "POST",
    headers: { "Content-Type": "application/json", "Idempotency-Key": crypto.randomUUID() },
    body: JSON.stringify(body),
  });
  const value = await response.json();
  if (!response.ok) throw new Error(value?.error?.message ?? value?.message ?? "Directory request failed.");
  return value.data;
}

function mappingStatus(position: Position) {
  if (!position.mapped) return "No GPO mapping";
  if (position.mapping_status === "blocked") return "Needs IT attention";
  if (position.mapping_status === "directory_applied") return "Directory configuration verified";
  return "Directory configuration pending";
}

function deliveryStatus(job: Job) {
  if (job.status === "directory_applied") return "Directory change verified";
  if (job.status === "awaiting_identity") return "Awaiting Directory account link";
  if (job.status === "blocked") return "Needs IT attention";
  if (job.status === "superseded") return "Superseded";
  return "Directory delivery in progress";
}

export function PositionGpoWorkspace({ holdingSlug, initial, connections, initialPositionId }: {
  holdingSlug: string;
  initial: Data;
  connections: Connection[];
  initialPositionId?: string;
}) {
  const selectedInitialPositionId = initial.positions.some((position) => position.id === initialPositionId) ? initialPositionId! : initial.positions[0]?.id ?? "";
  const initialPolicy = initial.policies.find((policy) => policy.position_id === selectedInitialPositionId);
  const [data, setData] = useState(initial);
  const [positionId, setPositionId] = useState(selectedInitialPositionId);
  const [detailOpen, setDetailOpen] = useState(Boolean(initialPositionId));
  const [activePane, setActivePane] = useState<"mapping" | "delivery" | "remove">("mapping");
  const [positionSearch, setPositionSearch] = useState("");
  const [statusFilter, setStatusFilter] = useState<"all" | "mapped" | "unmapped" | "attention">("all");
  const [connectionId, setConnectionId] = useState(initialPolicy?.connection_id ?? connections[0]?.id ?? "");
  const [gpoId, setGpoId] = useState(initialPolicy?.gpo_id ?? "");
  const [userFilterGroupId, setGroupId] = useState(initialPolicy?.user_filter_group_id ?? "");
  const [computerTargetOuId, setOuId] = useState(initialPolicy?.computer_target_ou_id ?? "");
  const [reason, setReason] = useState("");
  const [preview, setPreview] = useState<Preview | null>(null);
  const [unmapReason, setUnmapReason] = useState("");
  const [unmapPreview, setUnmapPreview] = useState<UnmapPreview | null>(null);
  const [retryTarget, setRetryTarget] = useState<{ kind: "config" } | { kind: "delivery"; job: Job } | null>(null);
  const [retryReason, setRetryReason] = useState("");
  const [message, setMessage] = useState("");
  const [messageError, setMessageError] = useState(false);
  const [busy, setBusy] = useState(false);

  const position = data.positions.find((item) => item.id === positionId);
  const policy = data.policies.find((item) => item.position_id === positionId);
  const jobs = data.jobs.filter((job) => job.position_id === positionId);
  const connection = connections.find((item) => item.id === connectionId);
  const objects = connection?.inventory?.objects ?? [];
  const selected = (kind: string) => objects.filter((object) => object.kind === kind);
  const policyConnection = connections.find((item) => item.id === policy?.connection_id);
  const policyObjectName = (id: string | null) => policyConnection?.inventory?.objects.find((object) => object.id === id)?.name ?? id ?? "None";
  const visiblePositions = data.positions.filter((item) =>
    item.title.toLocaleLowerCase().includes(positionSearch.trim().toLocaleLowerCase()) &&
    (statusFilter === "all" || (statusFilter === "mapped" && item.mapped) || (statusFilter === "unmapped" && !item.mapped) || (statusFilter === "attention" && item.mapping_status === "blocked")),
  );
  const root = "/api/v1/holdings/" + encodeURIComponent(holdingSlug) + "/technology/position-gpo";
  const input = { positionId, connectionId, gpoId, userFilterGroupId, computerTargetOuId, expectedVersion: policy?.version ?? 0, reason };
  const unmapInput = { positionId, expectedVersion: policy?.version ?? 0, reason: unmapReason };
  const reload = async () => {
    const response = await workFetch(root, { cache: "no-store" });
    if (!response.ok) throw new Error("Could not refresh position GPO status.");
    setData((await response.json()).data as Data);
  };
  function fail(error: unknown) {
    setMessage(error instanceof Error ? error.message : "The Directory request failed.");
    setMessageError(true);
  }
  function clearMessage() {
    setMessage("");
    setMessageError(false);
  }
  function choosePosition(id: string) {
    const nextPolicy = data.policies.find((item) => item.position_id === id);
    setPositionId(id);
    setConnectionId(nextPolicy?.connection_id ?? connections[0]?.id ?? "");
    setGpoId(nextPolicy?.gpo_id ?? "");
    setGroupId(nextPolicy?.user_filter_group_id ?? "");
    setOuId(nextPolicy?.computer_target_ou_id ?? "");
    setReason("");
    setUnmapReason("");
    setPreview(null);
    setUnmapPreview(null);
    setActivePane("mapping");
    setDetailOpen(true);
    clearMessage();
  }
  async function previewMapping() {
    setBusy(true);
    clearMessage();
    try { setPreview(await action(root + "/preview", input) as Preview); }
    catch (error) { fail(error); }
    finally { setBusy(false); }
  }
  async function publishMapping() {
    if (!preview) return;
    setBusy(true);
    clearMessage();
    try {
      await action(root + "/publish", { ...input, previewToken: preview.token });
      await reload();
      setPreview(null);
      setReason("");
      setMessage("Mapping published. Directory configuration and eligible holder delivery are queued.");
    } catch (error) { setPreview(null); fail(error); }
    finally { setBusy(false); }
  }
  async function previewRemoval() {
    setBusy(true);
    clearMessage();
    try { setUnmapPreview(await action(root + "/unmap-preview", unmapInput) as UnmapPreview); }
    catch (error) { fail(error); }
    finally { setBusy(false); }
  }
  async function removeMapping() {
    if (!unmapPreview) return;
    setBusy(true);
    clearMessage();
    try {
      await action(root + "/unmap", { ...unmapInput, previewToken: unmapPreview.token });
      await reload();
      setGpoId(""); setGroupId(""); setOuId("");
      setUnmapPreview(null); setUnmapReason("");
      setActivePane("mapping");
      setMessage("GPO mapping removed. Directory cleanup is queued; HR placement remains available.");
    } catch (error) { setUnmapPreview(null); fail(error); }
    finally { setBusy(false); }
  }
  async function retry() {
    if (!retryTarget || !policy || retryReason.trim().length < 3) return;
    setBusy(true);
    clearMessage();
    try {
      if (retryTarget.kind === "config") await action(root + "/retry-config", { positionId, expectedVersion: policy.version, reason: retryReason });
      else await action(root + "/retry", { id: retryTarget.job.id, expectedVersion: retryTarget.job.version, reason: retryReason });
      await reload();
      setRetryTarget(null);
      setRetryReason("");
      setMessage("Retry queued. Check the delivery status for verified read-back.");
    } catch (error) { fail(error); }
    finally { setBusy(false); }
  }

  return <section className={styles.workspace} data-detail-open={detailOpen} aria-label="Position GPO policies">
    <div className={styles.listPane}>
      <div className={styles.listHeader}><h2>GPO mappings by position</h2><span>{visiblePositions.length} of {data.positions.length}</span></div>
      <div className={styles.listControls}>
        <label>Search positions<input type="search" value={positionSearch} onChange={(event) => setPositionSearch(event.target.value)} /></label>
        <label>Status<select value={statusFilter} onChange={(event) => setStatusFilter(event.target.value as typeof statusFilter)}><option value="all">All statuses</option><option value="mapped">Mapped</option><option value="unmapped">No mapping</option><option value="attention">Needs attention</option></select></label>
      </div>
      <div className={styles.positionList} role="list">
        {visiblePositions.map((item) => <div key={item.id} role="listitem" className={styles.positionRow} data-selected={positionId === item.id}>
          <div><strong>{item.title}</strong><span>{item.holder_count} current holders · {mappingStatus(item)}</span></div>
          <button type="button" aria-pressed={positionId === item.id} onClick={() => choosePosition(item.id)}>{item.mapped ? "Edit GPO mapping" : "Map GPO"}</button>
        </div>)}
        {data.positions.length === 0 && <p className={styles.empty}>No active positions yet.</p>}
        {data.positions.length > 0 && visiblePositions.length === 0 && <p className={styles.empty}>No positions match these filters.</p>}
      </div>
    </div>

    <div className={styles.detailPane} id="position-gpo-editor">
      {position ? <>
        <button type="button" className={styles.backButton} onClick={() => setDetailOpen(false)}><ArrowLeft size={16} /> Back to positions</button>
        <div className={styles.detailHeader}><div><span className={styles.kicker}>Position GPO</span><h2>{position.title}</h2><p>{position.holder_count} current holders · {mappingStatus(position)}</p></div>{position.mapped && <span className={styles.badge}>Revision {policy?.revision ?? position.mapping_version}</span>}</div>
        <nav className={styles.detailTabs} aria-label="Position GPO details">
          <button type="button" aria-current={activePane === "mapping"} onClick={() => setActivePane("mapping")}>Mapping</button>
          <button type="button" aria-current={activePane === "delivery"} onClick={() => setActivePane("delivery")}>Delivery{jobs.length ? " · " + jobs.length : ""}</button>
          {policy?.mapped && <button type="button" aria-current={activePane === "remove"} onClick={() => setActivePane("remove")}>Remove</button>}
        </nav>
        {message && <p role={messageError ? "alert" : "status"} className={messageError ? styles.error : styles.message}>{message}</p>}
        {messageError && /Directory connection|Directory write/i.test(message) && <Link className={styles.helpLink} href={`/holdings/${encodeURIComponent(holdingSlug)}/people/settings?section=connections`}>Review Directory connection</Link>}

        {activePane === "mapping" && <div className={styles.detailContent}>
          <h2>Publish position GPO mapping</h2>
          <p className={styles.help}>Choose a GPO, user filter group, and computer target OU. Publishing queues Directory work. HR placement can continue while account linkage is pending.</p>
          {policy?.mapped && <div className={styles.currentMapping}>
            <h3>Current mapping</h3>
            <p>{policyConnection?.name ?? policy.connection_id ?? "None"} · GPO {policyObjectName(policy.gpo_id)}</p>
            <p>Group {policyObjectName(policy.user_filter_group_id)} · OU {policyObjectName(policy.computer_target_ou_id)}</p>
          </div>}
          <div className={styles.fields}>
            <label>Position<select value={positionId} onChange={(event) => choosePosition(event.target.value)}>{data.positions.map((item) => <option key={item.id} value={item.id}>{item.title}</option>)}</select></label>
            <label>Directory connection<select value={connectionId} onChange={(event) => { setConnectionId(event.target.value); setGpoId(""); setGroupId(""); setOuId(""); setPreview(null); }}>{connections.map((item) => <option key={item.id} value={item.id}>{item.name}</option>)}</select></label>
            <label>GPO<select value={gpoId} onChange={(event) => { setGpoId(event.target.value); setPreview(null); }}><option value="">Choose GPO</option>{selected("gpo").map((object) => <option key={object.id} value={object.id}>{object.name}</option>)}</select></label>
            <label>User filter group<select value={userFilterGroupId} onChange={(event) => { setGroupId(event.target.value); setPreview(null); }}><option value="">Choose group</option>{selected("group").map((object) => <option key={object.id} value={object.id}>{object.name}</option>)}</select></label>
            <label>Computer target OU<select value={computerTargetOuId} onChange={(event) => { setOuId(event.target.value); setPreview(null); }}><option value="">Choose OU</option>{selected("ou").map((object) => <option key={object.id} value={object.id}>{object.name}</option>)}</select></label>
            <label>Reason<input minLength={3} maxLength={1000} value={reason} onChange={(event) => { setReason(event.target.value); setPreview(null); }} /></label>
          </div>
          {!connections.length && <p className={styles.help}>Connect and read a Directory inventory before mapping a position.</p>}
          <button type="button" className="button primary" disabled={busy || !positionId || !connectionId || !gpoId || !userFilterGroupId || !computerTargetOuId || reason.trim().length < 3} onClick={() => void previewMapping()}>Preview impact</button>
        </div>}

        {activePane === "delivery" && <div className={styles.detailContent}>
          <h2>Directory configuration</h2>
          {policy?.mapped ? <div className={styles.currentMapping}><strong>Revision {policy.revision} · {policy.directory_config_status === "directory_applied" ? "Directory configuration verified" : policy.directory_config_status === "blocked" ? "Needs IT attention" : "Directory configuration pending"}</strong>
            <p>{policyConnection?.name ?? policy.connection_id ?? "None"} · GPO {policyObjectName(policy.gpo_id)} · Group {policyObjectName(policy.user_filter_group_id)} · OU {policyObjectName(policy.computer_target_ou_id)}</p>
            {policy.directory_config_error && <p role="status">{policy.directory_config_error}</p>}
            {policy.directory_config_status === "blocked" && <button type="button" className="button secondary" disabled={busy} onClick={() => { setRetryTarget({ kind: "config" }); setRetryReason(""); }}>Retry configuration</button>}
          </div> : <p className={styles.help}>No GPO mapping has been published for this position.</p>}
          <h2>Holder delivery</h2>
          <p className={styles.help}>A verified Directory change does not prove the policy reached a Windows endpoint.</p>
          {jobs.length === 0 && <p className={styles.empty}>No holder delivery jobs for this position.</p>}
          <div className={styles.jobs}>{jobs.map((job) => <article key={job.id}>
            <strong>{job.legal_name}</strong><span>{job.desired_action} · {deliveryStatus(job)}</span><small>Endpoint {job.endpoint_observed_at ? "observed" : "unconfirmed"}</small>
            {job.last_error && <p role="status">{job.last_error}</p>}
            {job.status === "blocked" && <button type="button" className="button secondary" disabled={busy} onClick={() => { setRetryTarget({ kind: "delivery", job }); setRetryReason(""); }}>Retry delivery</button>}
          </article>)}</div>
        </div>}

        {activePane === "remove" && policy?.mapped && <div className={styles.detailContent}>
          <h2>Remove GPO mapping</h2>
          <p className={styles.help}>HR can keep assigning this position. Removal queues cleanup of the Directory access this position owns. Shared mappings remain in place.</p>
          <label className={styles.removeReason}>Removal reason<input minLength={3} maxLength={1000} value={unmapReason} onChange={(event) => { setUnmapReason(event.target.value); setUnmapPreview(null); }} /></label>
          <button type="button" className="button secondary" disabled={busy || unmapReason.trim().length < 3} onClick={() => void previewRemoval()}>Review removal</button>
        </div>}
      </> : <div className={styles.emptyDetail}><h2>Position GPO</h2><p>Select a position to review or publish its mapping.</p></div>}
    </div>

    <Dialog open={!!preview} onClose={() => setPreview(null)} title="Preview GPO impact" presentation="responsive-panel">
      {preview && <div className={styles.dialogContent}>
        <p><strong>Revision {preview.revision}</strong> · {new Set(preview.holders.map((holder) => holder.engagement_id)).size} current holders</p>
        <p>{preview.changes.membersToAdd.length} linked account additions · {preview.holders.filter((holder) => holder.accountStatus === "awaiting_account_linkage").length} awaiting account linkage · {preview.changes.membersToRemoveFromPreviousGroup.length} removals from the previous filter group</p>
        {preview.sharedMappings.length > 0 && <p>This GPO is also mapped to {preview.sharedMappings.length} other positions. Their required groups and OU links remain combined.</p>}
        {preview.holders.length > 0 && <div className={styles.previewHolders}>{preview.holders.map((holder, index) => <p key={holder.engagement_id + "-" + index}>{holder.name} · {holder.accountStatus === "linked" ? "Directory account linked" : "Awaiting Directory account link"}</p>)}</div>}
        <p className={styles.help}>Directory inventory observed <TimeText value={preview.directory.observedAt} />. Endpoint application is tracked separately.</p>
        <button type="button" className="button primary" disabled={busy} onClick={() => void publishMapping()}>Publish revision</button>
      </div>}
    </Dialog>
    <Dialog open={!!unmapPreview} onClose={() => setUnmapPreview(null)} title="Review GPO removal" presentation="responsive-panel">
      {unmapPreview && <div className={styles.dialogContent}>
        <p>Remove revision {unmapPreview.revision} for <strong>{unmapPreview.title}</strong>. {unmapPreview.holders.length} current holders and {unmapPreview.holders.reduce((count, holder) => count + holder.account_ids.length, 0)} linked accounts will be reconciled.</p>
        {unmapPreview.sharedMappings.length > 0 && <p>This GPO remains mapped to {unmapPreview.sharedMappings.length} other positions. Their required groups and OU links stay in place.</p>}
        <p className={styles.help}>GPO {policyObjectName(unmapPreview.current.gpoId)} · Group {policyObjectName(unmapPreview.current.userFilterGroupId)} · OU {policyObjectName(unmapPreview.current.computerTargetOuId)}</p>
        <button type="button" className="button primary" disabled={busy} onClick={() => void removeMapping()}>Remove mapping</button>
      </div>}
    </Dialog>
    <Dialog open={!!retryTarget} onClose={() => setRetryTarget(null)} title={retryTarget?.kind === "config" ? "Retry Directory configuration" : "Retry holder delivery"} presentation="responsive-panel">
      {retryTarget && <div className={styles.dialogContent}><p>Give a reason for this retry. The result will be tracked through Directory read-back.</p><label>Reason<input minLength={3} maxLength={1000} value={retryReason} onChange={(event) => setRetryReason(event.target.value)} /></label>{messageError && message && <p role="alert" className={styles.error}>{message}</p>}<button type="button" className="button primary" disabled={busy || retryReason.trim().length < 3} onClick={() => void retry()}>Queue retry</button></div>}
    </Dialog>
  </section>;
}
