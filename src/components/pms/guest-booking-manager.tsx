"use client";
import { useEffect, useState } from "react";
import { money } from "@/lib/pms";
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
          if (!r.ok)
            throw new Error(j.error?.message ?? "Unable to load booking");
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
          <span className={styles.eyebrow}>
            {booking.state.replaceAll("_", " ")}
          </span>
          <h2>{booking.hotel.name}</h2>
          <p>Confirmation {booking.number}</p>
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
          <p>
            {booking.policy?.text ?? "Contact the hotel for booking terms."}
          </p>
          <p>
            {booking.hotel.address} · {booking.hotel.contactPhone} ·{" "}
            <a href={`mailto:${booking.hotel.contactEmail}`}>
              {booking.hotel.contactEmail}
            </a>
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
                    if (!r.ok)
                      throw new Error(
                        j.error?.message ?? "Cancellation failed",
                      );
                    setBooking(j.data);
                    setNotice(
                      "All rooms have been cancelled without a fee. A cancellation email is on its way.",
                    );
                    setConfirmCancel(false);
                  } catch (e) {
                    setError(
                      e instanceof Error ? e.message : "Please try again",
                    );
                  } finally {
                    setBusy(false);
                  }
                }}
              >
                <label className={styles.check}>
                  <input type="checkbox" required />
                  Cancel every room in this reservation
                </label>
                <div className={styles.actions}>
                  <button className={styles.primary} disabled={busy}>
                    Confirm cancellation
                  </button>
                  <button
                    className={styles.secondary}
                    type="button"
                    onClick={() => setConfirmCancel(false)}
                  >
                    Keep booking
                  </button>
                </div>
              </form>
            ) : (
              <button
                className={styles.secondary}
                onClick={() => setConfirmCancel(true)}
              >
                Cancel booking
              </button>
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
          Use the confirmation number and email from your reservation. No
          account is needed.
        </p>
        <div className={styles.grid}>
          <label>
            Confirmation number
            <input name="number" required maxLength={100} />
          </label>
          <label>
            Booking email
            <input name="email" type="email" required />
          </label>
        </div>
        <div className={styles.actions}>
          <button className={styles.primary} disabled={busy}>
            Email a private link
          </button>
        </div>
      </form>
    </div>
  );
}
