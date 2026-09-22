"use client";
import { useState, useRef } from "react";
import { records, type Snapshot, type Rate } from "@/lib/pms-model";
import { decimalToMinor, minorToDecimal } from "@/lib/finance-money";
import styles from "./booking.module.css";
type Save = (
  operation: string,
  input: unknown,
  version?: number,
) => Promise<boolean>;
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
    <details className={styles.surface}>
      <summary>Child prices · {rate.name}</summary>
      <p className={styles.small}>
        Each child counts toward capacity. The configured charge is added per
        child, per night, before the rate’s tax treatment. Ages without a band
        are not offered.
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
            setMessage(
              ok ? "Child prices saved" : "Not saved. Check the message above",
            );
          } finally {
            setBusy(false);
          }
        }}
      >
        {bands.map((b, i) => (
          <div className={styles.grid} key={i}>
            <label>
              From age
              <input
                type="number"
                min={0}
                max={17}
                required
                value={b.minAge}
                onChange={(e) =>
                  setBands(
                    bands.map((r, j) =>
                      j === i ? { ...r, minAge: Number(e.target.value) } : r,
                    ),
                  )
                }
              />
            </label>
            <label>
              Through age
              <input
                type="number"
                min={b.minAge}
                max={17}
                required
                value={b.maxAge}
                onChange={(e) =>
                  setBands(
                    bands.map((r, j) =>
                      j === i ? { ...r, maxAge: Number(e.target.value) } : r,
                    ),
                  )
                }
              />
            </label>
            <label>
              Charge per night (THB)
              <input
                type="number"
                min={0}
                step="0.01"
                required
                value={b.displayPrice}
                onChange={(e) =>
                  setBands(
                    bands.map((r, j) =>
                      j === i ? { ...r, displayPrice: e.target.value } : r,
                    ),
                  )
                }
              />
            </label>
            <button
              className={styles.secondary}
              type="button"
              onClick={() => setBands(bands.filter((_, j) => i !== j))}
            >
              Remove band
            </button>
          </div>
        ))}
        <div className={styles.actions}>
          <button
            className={styles.secondary}
            type="button"
            onClick={() =>
              setBands([
                ...bands,
                { minAge: 0, maxAge: 17, price: 0, displayPrice: "0.00" },
              ])
            }
            disabled={bands.length >= 18}
          >
            Add age band
          </button>
          <button className={styles.primary} disabled={busy}>
            Save child prices
          </button>
        </div>
        <p role="status">{message}</p>
      </form>
    </details>
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
          . Only selected rates and the room descriptions below are public. New
          hotels are unpublished.
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
                ok
                  ? "Booking website settings saved"
                  : "Not saved. Check the message above",
              );
            } finally {
              setBusy(false);
            }
          }}
        >
          <label className={styles.field}>
            Hotel introduction
            <textarea
              name="description"
              defaultValue={b?.description}
              required
              maxLength={3000}
            />
          </label>
          <div className={styles.grid}>
            <label>
              Public address
              <input
                name="address"
                defaultValue={b?.address}
                required
                maxLength={500}
              />
            </label>
            <label>
              Guest contact email
              <input
                type="email"
                name="email"
                defaultValue={b?.contactEmail}
                required
              />
            </label>
            <label>
              Guest contact phone
              <input name="phone" defaultValue={b?.contactPhone} required />
            </label>
            <label>
              Free cancellation: days before arrival
              <input
                type="number"
                name="days"
                min={0}
                max={365}
                defaultValue={b?.cancellationDays ?? 1}
                required
              />
            </label>
            <label>
              Deadline in hotel timezone
              <input
                type="time"
                name="time"
                defaultValue={b?.cancellationTime ?? "18:00"}
                required
              />
            </label>
          </div>
          <h3>Public rates</h3>
          {records(snapshot, "rate")
            .filter((r) => r.data.market === "public")
            .map((r) => (
              <label key={r.id} className={styles.check}>
                <input
                  type="checkbox"
                  name="rateIds"
                  value={r.id}
                  defaultChecked={b?.rateIds.includes(r.id)}
                />
                {r.data.name} ·{" "}
                {
                  records(snapshot, "room_type").find(
                    (t) => t.id === r.data.typeId,
                  )?.data.name
                }
              </label>
            ))}
          <h3>Room descriptions</h3>
          {records(snapshot, "room_type").map((r) => (
            <label className={styles.field} key={r.id}>
              {r.data.name}
              <textarea
                name={`room-${r.id}`}
                defaultValue={
                  b?.roomContent.find((c) => c.typeId === r.id)?.description
                }
                maxLength={2000}
                placeholder="Describe the room for guests. Required when its rates are selected."
              />
            </label>
          ))}
          <label className={styles.check}>
            <input
              type="checkbox"
              name="published"
              defaultChecked={b?.published ?? false}
            />
            Publish this hotel for online booking
          </label>
          <p className={styles.small}>
            Publishing requires an open hotel, selected rates, room descriptions
            and configured email delivery. Guests pay at the hotel. Late
            cancellations go to staff.
          </p>
          <button className={styles.primary} disabled={busy}>
            Save booking website settings
          </button>
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
