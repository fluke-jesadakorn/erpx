"use client";

import { useRouter } from "next/navigation";
import { useState } from "react";
import type { PlatformProvisioningSnapshot, ProvisionHoldingResult } from "@/server/platform-operations";
import type { ResourceSlot } from "@/lib/holding-registry";

export function PlatformHoldingProvisioning({ snapshot, canProvision }: { snapshot: PlatformProvisioningSnapshot; canProvision: boolean }) {
  const router = useRouter();
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState("");
  const [error, setError] = useState("");
  const [selected, setSelected] = useState<ResourceSlot | null>(snapshot.availableSlots[0] ?? null);

  async function submit(form: HTMLFormElement) {
    if (!selected || busy) return;
    setBusy(true); setMessage(""); setError("");
    const values = new FormData(form);
    try {
      const response = await fetch("/api/v1/platform/operations/provision", {
        method: "POST",
        headers: { "Content-Type": "application/json", "Idempotency-Key": crypto.randomUUID() },
        body: JSON.stringify({ holdingId: selected.id, slug: values.get("slug"), displayName: values.get("displayName"), reason: values.get("reason") }),
      });
      const body = await response.json() as { data?: ProvisionHoldingResult; error?: { message?: string } };
      if (!response.ok) throw new Error(body.error?.message ?? "Holding provisioning failed.");
      setMessage(`Activated ${body.data?.displayName ?? "Holding"}. Its company administrator can now complete setup.`);
      form.reset();
      router.refresh();
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : "Holding provisioning failed. Retry the same request.");
    } finally { setBusy(false); }
  }

  return <section className="card" id="platform-holding-provisioning">
    <h3>Provision a Holding</h3>
    <p className="muted">Activate a pre-provisioned database and private file namespace. This creates the control-plane binding; company setup and user invitations remain a separate reviewed step.</p>
    {message && <p role="status">{message}</p>}
    {error && <p role="alert" className="error">{error}</p>}
    {snapshot.rows.length > 0 && <div className="table-wrap"><table><thead><tr><th>Holding</th><th>State</th><th>Resource slot</th><th>Requested</th><th>Reason</th></tr></thead><tbody>{snapshot.rows.map((row) => <tr key={row.id}><td><strong>{row.displayName}</strong><br /><small>{row.slug}</small></td><td>{row.status}</td><td>{row.holdingId} · {row.writeBinding}</td><td>{new Date(row.requestedAt).toISOString()}</td><td>{row.reason}{row.error && <p className="error">{row.error}</p>}</td></tr>)}</tbody></table></div>}
    {canProvision && snapshot.availableSlots.length > 0 ? <form className="business-form" onSubmit={(event) => { event.preventDefault(); void submit(event.currentTarget); }}>
      <label>Resource slot<select required value={selected?.id ?? ""} onChange={(event) => setSelected(snapshot.availableSlots.find((slot) => slot.id === event.target.value) ?? null)}>{snapshot.availableSlots.map((slot) => <option key={slot.id} value={slot.id}>{slot.id} · {slot.write}</option>)}</select></label>
      <label>Company URL slug<input name="slug" pattern="[a-z][a-z0-9-]{2,39}" minLength={3} maxLength={40} required placeholder="acme" /></label>
      <label>Holding display name<input name="displayName" minLength={2} maxLength={120} required placeholder="Acme Group" /></label>
      <label>Activation reason<textarea name="reason" minLength={10} maxLength={1000} required placeholder="Reviewed customer onboarding request and verified the resource health." /></label>
      <button className="button" disabled={busy || !selected}>{busy ? "Activating…" : "Activate Holding"}</button>
    </form> : <p className="muted">{!canProvision ? "Only a Platform administrator can provision a Holding." : "No unbound, healthy resource slots are available."}</p>}
  </section>;
}
