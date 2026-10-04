"use client";
import { Button, Field, TextInput } from "@/components/ui/foundation";

import { useEffect, useState } from "react";
import { money } from "@/lib/pms-model";
import type { manageBooking } from "@/server/pms-booking";
import styles from "./booking.module.css";
type Booking = Awaited<ReturnType<typeof manageBooking>>;
export function GuestBookingManager({ slug }: { slug: string }) {
  const [token, setToken] = useState(""),
    [booking, setBooking] = useState<Booking | null>(null),
    [error, setError] = useState(""),
    [notice, setNotice] = useState(""),
    [busy, setBusy] = useState(false),
    [confirmCancel, setConfirmCancel] = useState(false);
  const base = `/api/public/booking/${encodeURIComponent(slug)}`;
  useEffect(() => {
    const key = `booking-access:${slug}`,
      fragment = new URLSearchParams(window.location.hash.slice(1)),
      value = fragment.get("token") ?? sessionStorage.getItem(key) ?? "";
    if (fragment.has("token")) {
      sessionStorage.setItem(key, value);
      history.replaceState(null, "", window.location.pathname);
    }
    if (value) {
      let active = true;
      fetch(`${base}/manage`, {
        method: "POST",
        headers: {
          "Content-Type": "application/json",
          Authorization: `Bearer ${value}`,
        },
        body: "{}",
      })
        .then(async (r) => {
          const j = await r.json();
          if (!r.ok) throw new Error(j.error?.message ?? "Unable to load booking");
          if (active) {
            setToken(value);
            setBooking(j.data);
          }
        })
        .catch((e) => {
          if (active) setError(e.message);
        });
      return () => {
        active = false;
      };
    }
  }, [base, slug]);
  return (
    <div className={styles.wrap}>
      {error && (
        <div className={styles.error} role="alert">
          {error}
        </div>
      )}
      {notice && (
        <div className={styles.notice} role="status">
          {notice}
        </div>
      )}
      {booking && (
        <section className={styles.surface}>
          <span className={styles.eyebrow}>{booking.state.replaceAll("_", " ")}</span>
          <h2>{booking.hotel.name}</h2>
          <p>Confirmation {booking.number}</p>
          {booking.refundState && <p>Refund status: {booking.refundState.replaceAll("_", " ")}</p>}
          <p>{booking.guest.name}</p>
          {booking.rooms.map((r, i) => (
            <div key={i} className={styles.offer}>
              <div>
                <h3>{r.roomName}</h3>
                <p>
                  {r.arrival} → {r.departure}
                </p>
                <p>
                  {r.adults} adult(s), {r.childAges.length} child(ren) ·{" "}
                  {r.state.replaceAll("_", " ")}
                </p>
              </div>
              <strong>
                {money(
                  r.nights.reduce((n, p) => n + p.total, 0),
                  booking.currency,
                )}
              </strong>
            </div>
          ))}
          <p>{booking.policy?.text ?? "Contact the hotel for booking terms."}</p>
          <p>
            {booking.hotel.address} · {booking.hotel.contactPhone} ·{" "}
            <a href={`mailto:${booking.hotel.contactEmail}`}>{booking.hotel.contactEmail}</a>
          </p>
          <p>{booking.cancellation.reason}</p>
          {booking.cancellation.allowed &&
            (confirmCancel ? (
              <form
                onSubmit={async (e) => {
                  e.preventDefault();
                  setBusy(true);
                  setError("");
                  try {
                    const r = await fetch(`${base}/cancel`, {
                      method: "POST",
                      headers: {
                        "Content-Type": "application/json",
                        Authorization: `Bearer ${token}`,
                      },
                      body: "{}",
                    });
                    const j = await r.json();
                    if (!r.ok) throw new Error(j.error?.message ?? "Cancellation failed");
                    setBooking(j.data);
                    setNotice(
                      "All rooms have been cancelled without a fee. Finance will approve the refund, and Stripe will confirm it. A cancellation email is on its way.",
                    );
                    setConfirmCancel(false);
                  } catch (e) {
                    setError(e instanceof Error ? e.message : "Please try again");
                  } finally {
                    setBusy(false);
                  }
                }}
              >
                <Field orientation="inline" className={styles.check}>
                  <TextInput type="checkbox" required />
                  Cancel every room in this reservation
                </Field>
                <div className={styles.actions}>
                  <Button
                    clientManaged type="submit"
                    variant="primary"
                    className={styles.primary}
                    disabled={busy}
                  >
                    Confirm cancellation
                  </Button>
                  <Button
                    variant="secondary"
                    className={styles.secondary}
                    type="button"
                    onClick={() => setConfirmCancel(false)}
                  >
                    Keep booking
                  </Button>
                </div>
              </form>
            ) : (
              <Button
                type="submit"
                variant="secondary"
                className={styles.secondary}
                onClick={() => setConfirmCancel(true)}
              >
                Cancel booking
              </Button>
            ))}
        </section>
      )}
      <form
        className={styles.surface}
        onSubmit={async (e) => {
          e.preventDefault();
          const f = new FormData(e.currentTarget);
          setBusy(true);
          setError("");
          try {
            const r = await fetch(`${base}/recover`, {
              method: "POST",
              headers: { "Content-Type": "application/json" },
              body: JSON.stringify({
                number: f.get("number"),
                email: f.get("email"),
              }),
            });
            const j = await r.json();
            if (!r.ok) throw new Error(j.error?.message ?? "Please try again");
            setNotice(j.data.message);
          } catch (e) {
            setError(e instanceof Error ? e.message : "Please try again");
          } finally {
            setBusy(false);
          }
        }}
      >
        <h2>Request a booking access link</h2>
        <p className={styles.small}>
          Use the confirmation number and email from your reservation. No account is needed.
        </p>
        <div className={styles.grid}>
          <Field>
            Confirmation number
            <TextInput name="number" required maxLength={100} />
          </Field>
          <Field>
            Booking email
            <TextInput name="email" type="email" required />
          </Field>
        </div>
        <div className={styles.actions}>
          <Button clientManaged type="submit" variant="primary" className={styles.primary} disabled={busy}>
            Email a private link
          </Button>
        </div>
      </form>
    </div>
  );
}
