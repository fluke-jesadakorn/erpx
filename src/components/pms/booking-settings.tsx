"use client";
import { FormDisclosure } from "@/components/ui/form-disclosure";
import { Button, Field, TextArea, TextInput } from "@/components/ui/foundation";

import { useState, useRef } from "react";
import { records, type Snapshot, type Rate } from "@/lib/pms-model";
import { decimalToMinor, minorToDecimal } from "@/lib/finance-money";
import styles from "./booking.module.css";
type Save = (operation: string, input: unknown, version?: number) => Promise<boolean>;
function ChildPrices({
  id,
  rate,
  version,
  save,
}: {
  id: string;
  rate: Rate;
  version: number;
  save: Save;
}) {
  const [bands, setBands] = useState(
      (rate.childBands ?? []).map((b) => ({
        ...b,
        displayPrice: minorToDecimal(b.price),
      })),
    ),
    [busy, setBusy] = useState(false),
    [message, setMessage] = useState("");
  const editingVersion = useRef<number | undefined>(undefined);
  return (
    <FormDisclosure className={styles.surface}>
      <summary>Child prices · {rate.name}</summary>
      <p className={styles.small}>
        Each child counts toward capacity. The configured charge is added per child, per night,
        before the rate’s tax treatment. Ages without a band are not offered.
      </p>
      <form
        onChange={() => {
          editingVersion.current ??= version;
        }}
        onClickCapture={() => {
          editingVersion.current ??= version;
        }}
        onSubmit={async (e) => {
          e.preventDefault();
          setBusy(true);
          try {
            const ok = await save(
              "rate.save",
              {
                ...rate,
                id,
                childBands: bands.map(({ displayPrice, ...b }) => ({
                  ...b,
                  price: decimalToMinor(displayPrice),
                })),
              },
              editingVersion.current ?? version,
            );
            if (ok) editingVersion.current = undefined;
            setMessage(ok ? "Child prices saved" : "Not saved. Check the message above");
          } finally {
            setBusy(false);
          }
        }}
      >
        {bands.map((b, i) => (
          <div className={styles.grid} key={i}>
            <Field>
              From age
              <TextInput
                type="number"
                min={0}
                max={17}
                required
                value={b.minAge}
                onChange={(e) =>
                  setBands(
                    bands.map((r, j) => (j === i ? { ...r, minAge: Number(e.target.value) } : r)),
                  )
                }
              />
            </Field>
            <Field>
              Through age
              <TextInput
                type="number"
                min={b.minAge}
                max={17}
                required
                value={b.maxAge}
                onChange={(e) =>
                  setBands(
                    bands.map((r, j) => (j === i ? { ...r, maxAge: Number(e.target.value) } : r)),
                  )
                }
              />
            </Field>
            <Field>
              Charge per night (THB)
              <TextInput
                type="number"
                min={0}
                step="0.01"
                required
                value={b.displayPrice}
                onChange={(e) =>
                  setBands(
                    bands.map((r, j) => (j === i ? { ...r, displayPrice: e.target.value } : r)),
                  )
                }
              />
            </Field>
            <Button
              variant="secondary"
              className={styles.secondary}
              type="button"
              onClick={() => setBands(bands.filter((_, j) => i !== j))}
            >
              Remove band
            </Button>
          </div>
        ))}
        <div className={styles.actions}>
          <Button
            variant="secondary"
            className={styles.secondary}
            type="button"
            onClick={() =>
              setBands([...bands, { minAge: 0, maxAge: 17, price: 0, displayPrice: "0.00" }])
            }
            disabled={bands.length >= 18}
          >
            Add age band
          </Button>
          <Button clientManaged type="submit" variant="primary" className={styles.primary} disabled={busy}>
            Save child prices
          </Button>
        </div>
        <p role="status">{message}</p>
      </form>
    </FormDisclosure>
  );
}
export function BookingSettings({
  snapshot,
  save,
  slug,
}: {
  snapshot: Snapshot;
  save: Save;
  slug: string;
}) {
  const b = snapshot.property.config.booking,
    [busy, setBusy] = useState(false),
    [message, setMessage] = useState("");
  const editingVersion = useRef<number | undefined>(undefined);
  return (
    <div className={styles.wrap}>
      <section className={styles.surface}>
        <h2>Guest booking website</h2>
        <p className={styles.small}>
          Publish this hotel on the{" "}
          <a href={`/book/${slug}`} target="_blank" rel="noreferrer">
            shared booking website
          </a>
          . Only selected rates and the room descriptions below are public. New hotels are
          unpublished.
        </p>
        <form
          onChange={() => {
            editingVersion.current ??= snapshot.property.version;
          }}
          onSubmit={async (e) => {
            e.preventDefault();
            const f = new FormData(e.currentTarget);
            setBusy(true);
            try {
              const rateIds = f.getAll("rateIds").map(String),
                typeIds = new Set(
                  records(snapshot, "rate")
                    .filter((r) => rateIds.includes(r.id))
                    .map((r) => r.data.typeId),
                );
              const ok = await save(
                "booking.save",
                {
                  published: f.get("published") === "on",
                  description: String(f.get("description")),
                  address: String(f.get("address")),
                  contactEmail: String(f.get("email")),
                  contactPhone: String(f.get("phone")),
                  cancellationDays: Number(f.get("days")),
                  cancellationTime: String(f.get("time")),
                  rateIds,
                  roomContent: records(snapshot, "room_type")
                    .filter((r) => typeIds.has(r.id))
                    .map((r) => ({
                      typeId: r.id,
                      description: String(f.get(`room-${r.id}`)),
                    })),
                },
                editingVersion.current ?? snapshot.property.version,
              );
              if (ok) editingVersion.current = undefined;
              setMessage(
                ok ? "Booking website settings saved" : "Not saved. Check the message above",
              );
            } finally {
              setBusy(false);
            }
          }}
        >
          <Field className={styles.field}>
            Hotel introduction
            <TextArea name="description" defaultValue={b?.description} required maxLength={3000} />
          </Field>
          <div className={styles.grid}>
            <Field>
              Public address
              <TextInput name="address" defaultValue={b?.address} required maxLength={500} />
            </Field>
            <Field>
              Guest contact email
              <TextInput type="email" name="email" defaultValue={b?.contactEmail} required />
            </Field>
            <Field>
              Guest contact phone
              <TextInput name="phone" defaultValue={b?.contactPhone} required />
            </Field>
            <Field>
              Free cancellation: days before arrival
              <TextInput
                type="number"
                name="days"
                min={0}
                max={365}
                defaultValue={b?.cancellationDays ?? 1}
                required
              />
            </Field>
            <Field>
              Deadline in hotel timezone
              <TextInput
                type="time"
                name="time"
                defaultValue={b?.cancellationTime ?? "18:00"}
                required
              />
            </Field>
          </div>
          <h3>Public rates</h3>
          {records(snapshot, "rate")
            .filter((r) => r.data.market === "public")
            .map((r) => (
              <Field orientation="inline" key={r.id} className={styles.check}>
                <TextInput
                  type="checkbox"
                  name="rateIds"
                  value={r.id}
                  defaultChecked={b?.rateIds.includes(r.id)}
                />
                {r.data.name} ·{" "}
                {records(snapshot, "room_type").find((t) => t.id === r.data.typeId)?.data.name}
              </Field>
            ))}
          <h3>Room descriptions</h3>
          {records(snapshot, "room_type").map((r) => (
            <Field className={styles.field} key={r.id}>
              {r.data.name}
              <TextArea
                name={`room-${r.id}`}
                defaultValue={b?.roomContent.find((c) => c.typeId === r.id)?.description}
                maxLength={2000}
                placeholder="Describe the room for guests. Required when its rates are selected."
              />
            </Field>
          ))}
          <Field orientation="inline" className={styles.check}>
            <TextInput type="checkbox" name="published" defaultChecked={b?.published ?? false} />
            Publish this hotel for online booking
          </Field>
          <p className={styles.small}>
            Publishing requires an open hotel, selected rates, room descriptions and configured
            email delivery. Guests pay at the hotel. Late cancellations go to staff.
          </p>
          <Button clientManaged type="submit" variant="primary" className={styles.primary} disabled={busy}>
            Save booking website settings
          </Button>
          <p role="status">{message}</p>
        </form>
      </section>
      {records(snapshot, "rate").map((r) => (
        <ChildPrices
          key={`${r.id}-${r.version}`}
          id={r.id}
          rate={r.data}
          version={snapshot.property.version}
          save={save}
        />
      ))}
    </div>
  );
}
