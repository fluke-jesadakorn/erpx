"use client";
import { CollectionRecordLink, RecordCollectionDrawer } from "@/components/records/record-collection";
import { InlineNotice } from "@/components/ui/foundation";

import { Button, Field, Select, Table, TextArea, TextInput } from "@/components/ui/foundation";

import { TimeText } from "@/components/time/time-preference-context";

import { useRouter } from "next/navigation";
import { workFetch } from "@/lib/work-fetch";
import { useState } from "react";
import type {
  PlatformProvisioningSnapshot,
  ProvisionHoldingResult,
} from "@/server/modules/platform_administration/public/platform-operations";
import type { ResourceSlot } from "@/lib/holding-registry";
import { Card, TableFrame, FormLayout } from "@/components/ui/foundation";

export function PlatformHoldingProvisioning({
  snapshot,
  canProvision,
}: {
  snapshot: PlatformProvisioningSnapshot;
  canProvision: boolean;
}) {
  const router = useRouter();
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState("");
  const [error, setError] = useState("");
  const [selected, setSelected] = useState<ResourceSlot | null>(snapshot.availableSlots[0] ?? null);

  async function submit(form: HTMLFormElement) {
    if (!selected || busy) return;
    setBusy(true);
    setMessage("");
    setError("");
    const values = new FormData(form);
    try {
      const response = await workFetch("/api/v1/platform/operations/provision", {
        method: "POST",
        headers: { "Content-Type": "application/json", "Idempotency-Key": crypto.randomUUID() },
        body: JSON.stringify({
          holdingId: selected.id,
          slug: values.get("slug"),
          displayName: values.get("displayName"),
          reason: values.get("reason"),
        }),
      });
      const body = (await response.json()) as {
        data?: ProvisionHoldingResult;
        error?: { message?: string };
      };
      if (!response.ok) throw new Error(body.error?.message ?? "Holding provisioning failed.");
      setMessage(
        `Activated ${body.data?.displayName ?? "Holding"}. Its company administrator can now complete setup.`,
      );
      form.reset();
      router.refresh();
    } catch (cause) {
      setError(
        cause instanceof Error
          ? cause.message
          : "Holding provisioning failed. Retry the same request.",
      );
    } finally {
      setBusy(false);
    }
  }

  return (
    <Card as="section" appearance="standard" className="ui-panel" id="platform-holding-provisioning">
      <RecordCollectionDrawer<PlatformProvisioningSnapshot> kind="platform_provision" title="Holding provisioning" endpoint="/api/v1/platform/operations/provision" select={(snapshot,id)=>snapshot.rows.find(row=>row.id===id) as unknown as Record<string,unknown>??null}/>
      <h3>Provision a Holding</h3>
      <p className="muted">
        Activate a pre-provisioned database and private file namespace. This creates the
        control-plane binding; company setup and user invitations remain a separate reviewed step.
      </p>
      {message && <InlineNotice tone="info" role="status">{message}</InlineNotice>}
      {error && (
        <InlineNotice tone="danger" role="alert" className="error">
          {error}
        </InlineNotice>
      )}
      {snapshot.rows.length > 0 && (

          <TableFrame appearance="standard" label="Records"><Table>
            <thead>
              <tr>
                <th>Holding</th>
                <th>State</th>
                <th>Resource slot</th>
                <th>Requested</th>
                <th>Reason</th>
              </tr>
            </thead>
            <tbody>
              {snapshot.rows.map((row) => (
                <tr key={row.id}>
                  <td>
                    <CollectionRecordLink kind="platform_provision" id={row.id}>{row.displayName}</CollectionRecordLink>
                    <br />
                    <small>{row.slug}</small>
                  </td>
                  <td>{row.status}</td>
                  <td>
                    {row.holdingId} · {row.writeBinding}
                  </td>
                  <td>
                    <TimeText appearance="standard" value={row.requestedAt} />
                  </td>
                  <td>
                    {row.reason}
                    {row.error && <p className="error">{row.error}</p>}
                  </td>
                </tr>
              ))}
            </tbody>
          </Table></TableFrame>

      )}
      {canProvision && snapshot.availableSlots.length > 0 ? (
        <FormLayout
          className="ui-form-grid"
          onSubmit={(event) => {
            event.preventDefault();
            void submit(event.currentTarget);
          }}
        >
          <Field appearance="standard">
            Resource slot
            <Select appearance="standard"
              required
              value={selected?.id ?? ""}
              onChange={(event) =>
                setSelected(
                  snapshot.availableSlots.find((slot) => slot.id === event.target.value) ?? null,
                )
              }
            >
              {snapshot.availableSlots.map((slot) => (
                <option key={slot.id} value={slot.id}>
                  {slot.id} · {slot.write}
                </option>
              ))}
            </Select>
          </Field>
          <Field appearance="standard">
            Company URL slug
            <TextInput appearance="standard"
              name="slug"
              pattern="[a-z][a-z0-9-]{2,39}"
              minLength={3}
              maxLength={40}
              required
              placeholder="acme"
            />
          </Field>
          <Field appearance="standard">
            Holding display name
            <TextInput appearance="standard"
              name="displayName"
              minLength={2}
              maxLength={120}
              required
              placeholder="Acme Group"
            />
          </Field>
          <Field appearance="standard">
            Activation reason
            <TextArea appearance="standard"
              name="reason"
              minLength={10}
              maxLength={1000}
              required
              placeholder="Reviewed customer onboarding request and verified the resource health."
            />
          </Field>
          <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy || !selected}>
            {busy ? "Activating…" : "Activate Holding"}
          </Button>
        </FormLayout>
      ) : (
        <p className="muted">
          {!canProvision
            ? "Only a Platform administrator can provision a Holding."
            : "No unbound, healthy resource slots are available."}
        </p>
      )}
    </Card>
  );
}
