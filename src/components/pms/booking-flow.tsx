"use client";
import { useRef, useState } from "react";
import { money, plusDays, type Guest } from "@/lib/pms-model";
import type { BookingQuote, RoomSelection } from "@/lib/pms-booking";
import { workFetch } from "@/lib/work-fetch";
import styles from "./booking.module.css";
export type HotelCard = {
  id: string;
  name: string;
  description: string;
  address: string;
  contactEmail: string;
  contactPhone: string;
  currency: string;
  checkIn: string;
  checkOut: string;
  timezone: string;
  roomContent: Array<{ typeId: string; description: string }>;
};
type Result = { hotel: HotelCard; offers: BookingQuote[] };
type Staff = {
  base: string;
  propertyId: string;
  guests: Array<{ id: string; data: Guest }>;
  blocks: Array<{ id: string; name: string }>;
  save: (input: unknown) => Promise<boolean>;
};
async function jsonRequest(
  url: string,
  input: unknown,
  staff = false,
  headers: Record<string, string> = {},
) {
  const response = await (staff ? workFetch : fetch)(url, {
    method: "POST",
    headers: { "Content-Type": "application/json", ...headers },
    body: JSON.stringify(input),
  });
  const data = await response.json();
  if (!response.ok)
    throw new Error(data.error?.message ?? "Request failed. Please try again");
  return data.data;
}
export function QuoteDetails({
  quote,
  confirmed = false,
}: {
  quote: BookingQuote;
  confirmed?: boolean;
}) {
  return (
    <>
      <div className={styles.scroll}>
        <table className={styles.table}>
          <thead>
            <tr>
              <th>Room / night</th>
              <th>Rooms</th>
              <th>Room charge</th>
              <th>Service</th>
              <th>Tax</th>
              <th>Total</th>
            </tr>
          </thead>
          <tbody>
            {quote.lines.flatMap((line, i) =>
              line.nights.map((n) => (
                <tr key={`${i}-${n.date}`}>
                  <td>
                    {line.roomName} · {n.date}
                  </td>
                  <td>{line.quantity}</td>
                  <td>{money(n.net * line.quantity, quote.currency)}</td>
                  <td>{money(n.service * line.quantity, quote.currency)}</td>
                  <td>{money(n.tax * line.quantity, quote.currency)}</td>
                  <td>{money(n.total * line.quantity, quote.currency)}</td>
                </tr>
              )),
            )}
          </tbody>
        </table>
      </div>
      <p className={styles.total}>
        Stay total {money(quote.total, quote.currency)}
      </p>
      <p>
        {quote.policy?.text ??
          "Contact the hotel for cancellation terms. Payment is due at the hotel."}
      </p>
      {!confirmed && (
        <p className={styles.small}>
          Availability is checked again when you confirm. Searching does not
          hold a room.
        </p>
      )}
    </>
  );
}
export function BookingFlow({
  slug,
  hotels = [],
  propertyId,
  today,
  staff,
}: {
  slug: string;
  hotels?: HotelCard[];
  propertyId?: string;
  today: string;
  staff?: Staff;
}) {
  const [step, setStep] = useState(0),
    [results, setResults] = useState<Result[] | null>(null),
    [rooms, setRooms] = useState<RoomSelection[]>([]),
    [selectedHotel, setHotel] = useState(propertyId ?? staff?.propertyId ?? "");
  const [quote, setQuote] = useState<BookingQuote | null>(null),
    [quoteToken, setQuoteToken] = useState(""),
    [guest, setGuest] = useState({ name: "", email: "", phone: "" }),
    [guestId, setGuestId] = useState("");
  const [notes, setNotes] = useState(""),
    [source, setSource] = useState("direct"),
    [externalRef, setExternalRef] = useState(""),
    [sourceName, setSourceName] = useState(""),
    [groupName, setGroupName] = useState(""),
    [blockId, setBlockId] = useState("");
  const [busy, setBusy] = useState(false),
    [error, setError] = useState(""),
    [confirmation, setConfirmation] = useState<{ number?: string } | null>(
      null,
    );
  const attempt = useRef<{ body: string; key: string } | null>(null);
  const base = `/api/public/booking/${encodeURIComponent(slug)}`;
  async function action(fn: () => Promise<void>) {
    setBusy(true);
    setError("");
    try {
      await fn();
    } catch (e) {
      setError(e instanceof Error ? e.message : "Please try again");
    } finally {
      setBusy(false);
    }
  }
  async function review() {
    if (staff) {
      const q = await jsonRequest(
        `/api/v1${staff.base}/pms/quote`,
        { propertyId: staff.propertyId, input: { rooms, blockId } },
        true,
      );
      setQuote(q);
    } else {
      const q = await jsonRequest(`${base}/quote`, {
        propertyId: selectedHotel,
        rooms,
      });
      setQuote(q.quote);
      setQuoteToken(q.token);
    }
    setStep(2);
  }
  return (
    <div className={styles.wrap}>
      <ol className={styles.steps}>
        {["1 · Find rooms", "2 · Guest details", "3 · Review & confirm"].map(
          (text, i) => (
            <li key={text} aria-current={step === i ? "step" : undefined}>
              {text}
            </li>
          ),
        )}
      </ol>
      {error && (
        <div className={styles.error} role="alert">
          {error}
        </div>
      )}
      {confirmation ? (
        <div className={styles.notice} role="status">
          <h2>
            Booking confirmed
            {confirmation.number ? ` · ${confirmation.number}` : ""}
          </h2>
          <p>
            {staff
              ? "The reservation is now available in the list below."
              : "Payment is due at the hotel. Your email will contain the confirmation and a private link to manage your booking."}
          </p>
          {quote && <QuoteDetails quote={quote} confirmed />}
          <button
            className={styles.secondary}
            onClick={() => {
              setConfirmation(null);
              setRooms([]);
              setQuote(null);
              setResults(null);
              setStep(0);
              attempt.current = null;
            }}
          >
            Book another stay
          </button>
        </div>
      ) : (
        <>
          {step === 0 && (
            <>
              <form
                className={styles.surface}
                onSubmit={(e) => {
                  e.preventDefault();
                  const form = new FormData(e.currentTarget);
                  void action(async () => {
                    const childAges = String(form.get("children") ?? "")
                      .trim()
                      .split(/[,\s]+/)
                      .filter(Boolean)
                      .map(Number);
                    const search = {
                      arrival: String(form.get("arrival")),
                      departure: String(form.get("departure")),
                      adults: Number(form.get("adults")),
                      childAges,
                      quantity: Number(form.get("quantity")),
                      blockId,
                    };
                    setRooms([]);
                    setQuote(null);
                    const hotelId =
                      staff?.propertyId ??
                      String(form.get("hotel") ?? propertyId ?? "");
                    if (staff) {
                      const offers = await jsonRequest(
                        `/api/v1${staff.base}/pms/availability`,
                        { propertyId: staff.propertyId, input: search },
                        true,
                      );
                      setResults([
                        {
                          hotel: {
                            id: staff.propertyId,
                            name: "Available rooms",
                            description: "",
                            address: "",
                            contactEmail: "",
                            contactPhone: "",
                            currency: "THB",
                            checkIn: "",
                            checkOut: "",
                            timezone: "",
                            roomContent: [],
                          },
                          offers,
                        },
                      ]);
                    } else
                      setResults(
                        await jsonRequest(`${base}/availability`, {
                          search,
                          ...(hotelId ? { propertyId: hotelId } : {}),
                        }),
                      );
                  });
                }}
              >
                <div className={styles.grid}>
                  {!staff && !propertyId && (
                    <label>
                      Hotel
                      <select name="hotel">
                        <option value="">All hotels</option>
                        {hotels.map((h) => (
                          <option key={h.id} value={h.id}>
                            {h.name}
                          </option>
                        ))}
                      </select>
                    </label>
                  )}
                  {staff && (
                    <label>
                      Group allocation
                      <select
                        value={blockId}
                        onChange={(e) => setBlockId(e.target.value)}
                      >
                        <option value="">None</option>
                        {staff.blocks.map((b) => (
                          <option key={b.id} value={b.id}>
                            {b.name}
                          </option>
                        ))}
                      </select>
                    </label>
                  )}
                  <label>
                    Check-in
                    <input
                      name="arrival"
                      type="date"
                      min={today}
                      defaultValue={today}
                      required
                    />
                  </label>
                  <label>
                    Check-out
                    <input
                      name="departure"
                      type="date"
                      min={today}
                      defaultValue={plusDays(today, 1)}
                      required
                    />
                  </label>
                  <label>
                    Adults per room
                    <input
                      name="adults"
                      type="number"
                      min={1}
                      max={50}
                      defaultValue={2}
                      required
                    />
                  </label>
                  <label>
                    Child ages per room
                    <input
                      name="children"
                      placeholder="e.g. 4, 9"
                      aria-describedby="child-help"
                    />
                  </label>
                  <label>
                    Rooms per selection
                    <input
                      name="quantity"
                      type="number"
                      min={1}
                      max={20}
                      defaultValue={1}
                      required
                    />
                  </label>
                </div>
                <p id="child-help" className={styles.small}>
                  Leave child ages blank for adults only. Every guest counts
                  toward room capacity.
                </p>
                <button className={styles.primary} disabled={busy}>
                  Search availability
                </button>
              </form>
              {results && (
                <div className={styles.surface}>
                  <h2>Choose your rooms</h2>
                  {!results.some((r) => r.offers.length) && (
                    <p>
                      No rooms match these dates and guests. Try different dates
                      or contact the hotel.
                    </p>
                  )}
                  {results.map((result) => (
                    <div key={result.hotel.id}>
                      <div className={styles.hotel}>
                        <h3>{result.hotel.name}</h3>
                        <p className={styles.small}>{result.hotel.address}</p>
                      </div>
                      {result.offers.map((q) => {
                        const line = q.lines[0];
                        return (
                          <div
                            className={styles.offer}
                            key={`${result.hotel.id}-${line.rateId}`}
                          >
                            <div>
                              <h3>{line.roomName}</h3>
                              <p>
                                {line.rateName} · {line.quantity} room(s) ·{" "}
                                {line.nights.length} night(s)
                              </p>
                              <p>
                                {
                                  result.hotel.roomContent.find(
                                    (c) => c.typeId === line.typeId,
                                  )?.description
                                }
                              </p>
                              <p>{line.inclusions}</p>
                              <p className={styles.small}>{q.policy?.text}</p>
                            </div>
                            <div>
                              <div className={styles.price}>
                                {money(q.total, q.currency)}
                              </div>
                              <p className={styles.small}>
                                Entire stay, including tax & service
                              </p>
                              <button
                                type="button"
                                className={styles.secondary}
                                disabled={busy}
                                onClick={() => {
                                  if (
                                    rooms.length &&
                                    selectedHotel !== result.hotel.id
                                  ) {
                                    setError(
                                      "A reservation belongs to one hotel. Remove selected rooms to switch hotels.",
                                    );
                                    return;
                                  }
                                  setError("");
                                  setHotel(result.hotel.id);
                                  setRooms([
                                    ...rooms,
                                    {
                                      typeId: line.typeId,
                                      rateId: line.rateId,
                                      arrival: line.arrival,
                                      departure: line.departure,
                                      adults: line.adults,
                                      childAges: line.childAges,
                                      quantity: line.quantity,
                                    },
                                  ]);
                                }}
                              >
                                Add rooms
                              </button>
                            </div>
                          </div>
                        );
                      })}
                    </div>
                  ))}
                </div>
              )}
              {rooms.length > 0 && (
                <div className={styles.surface}>
                  <h3>Your selection</h3>
                  {rooms.map((r, i) => (
                    <div className={styles.offer} key={i}>
                      <span>
                        {
                          results
                            ?.flatMap((x) => x.offers)
                            .find((q) => q.lines[0].rateId === r.rateId)
                            ?.lines[0].roomName
                        }{" "}
                        · {r.quantity} room(s) · {r.arrival} → {r.departure}
                      </span>
                      <button
                        className={styles.secondary}
                        onClick={() =>
                          setRooms(rooms.filter((_, index) => index !== i))
                        }
                      >
                        Remove
                      </button>
                    </div>
                  ))}
                  <button
                    className={styles.primary}
                    disabled={busy}
                    onClick={() => setStep(1)}
                  >
                    Continue to guest details
                  </button>
                </div>
              )}
            </>
          )}
          {step === 1 && (
            <form
              className={styles.surface}
              onSubmit={(e) => {
                e.preventDefault();
                void action(review);
              }}
            >
              <h2>Guest details</h2>
              {staff && (
                <label className={styles.field}>
                  Guest profile
                  <select
                    value={guestId}
                    onChange={(e) => setGuestId(e.target.value)}
                  >
                    <option value="">Create a guest with this booking</option>
                    {staff.guests.map((g) => (
                      <option key={g.id} value={g.id}>
                        {g.data.name} · {g.data.email}
                      </option>
                    ))}
                  </select>
                </label>
              )}
              {!guestId && (
                <div className={styles.grid}>
                  <label>
                    Full name
                    <input
                      required
                      value={guest.name}
                      maxLength={200}
                      autoComplete="name"
                      onChange={(e) =>
                        setGuest({ ...guest, name: e.target.value })
                      }
                    />
                  </label>
                  <label>
                    Email
                    <input
                      required
                      type="email"
                      value={guest.email}
                      autoComplete="email"
                      onChange={(e) =>
                        setGuest({ ...guest, email: e.target.value })
                      }
                    />
                  </label>
                  <label>
                    Phone
                    <input
                      required
                      type="tel"
                      value={guest.phone}
                      maxLength={80}
                      autoComplete="tel"
                      onChange={(e) =>
                        setGuest({ ...guest, phone: e.target.value })
                      }
                    />
                  </label>
                </div>
              )}
              <label className={styles.field}>
                Special requests
                <textarea
                  value={notes}
                  maxLength={2000}
                  onChange={(e) => setNotes(e.target.value)}
                  placeholder="Requests are subject to hotel confirmation"
                />
              </label>
              {staff && (
                <div className={styles.grid}>
                  <label>
                    Booking source
                    <select
                      value={source}
                      onChange={(e) => setSource(e.target.value)}
                    >
                      {[
                        "direct",
                        "phone",
                        "walk_in",
                        "Booking.com",
                        "Agoda",
                        "Expedia",
                        "other_ota",
                      ].map((s) => (
                        <option key={s}>{s}</option>
                      ))}
                    </select>
                  </label>
                  {source === "other_ota" && (
                    <label>
                      OTA name
                      <input
                        required
                        value={sourceName}
                        onChange={(e) => setSourceName(e.target.value)}
                      />
                    </label>
                  )}
                  <label>
                    Group name
                    <input
                      value={groupName}
                      onChange={(e) => setGroupName(e.target.value)}
                    />
                  </label>
                  <label>
                    External confirmation
                    <input
                      value={externalRef}
                      onChange={(e) => setExternalRef(e.target.value)}
                    />
                  </label>
                  <label>
                    Group allocation
                    <select
                      value={blockId}
                      onChange={(e) => setBlockId(e.target.value)}
                    >
                      <option value="">None</option>
                      {staff.blocks.map((b) => (
                        <option key={b.id} value={b.id}>
                          {b.name}
                        </option>
                      ))}
                    </select>
                  </label>
                </div>
              )}
              <div className={styles.actions}>
                <button
                  type="button"
                  className={styles.secondary}
                  onClick={() => setStep(0)}
                >
                  Back to rooms
                </button>
                <button className={styles.primary} disabled={busy}>
                  Review price & policy
                </button>
              </div>
            </form>
          )}
          {step === 2 && quote && (
            <form
              className={styles.surface}
              onSubmit={(e) => {
                e.preventDefault();
                void action(async () => {
                  if (staff) {
                    const ok = await staff.save({
                      ...(guestId ? { guestId } : { guest }),
                      source:
                        source === "other_ota" ? sourceName.trim() : source,
                      groupName,
                      externalRef,
                      blockId,
                      notes,
                      rooms,
                      quote: JSON.stringify(quote),
                    });
                    if (!ok)
                      throw new Error(
                        "The booking was not saved. Review the message above, then get a fresh quote.",
                      );
                    setConfirmation({});
                  } else {
                    const input = {
                      propertyId: selectedHotel,
                      guest,
                      notes,
                      quoteToken,
                    };
                    const body = JSON.stringify(input);
                    if (attempt.current?.body !== body)
                      attempt.current = { body, key: crypto.randomUUID() };
                    const result = await jsonRequest(
                      `${base}/reservations`,
                      input,
                      false,
                      { "Idempotency-Key": attempt.current.key },
                    );
                    setConfirmation(result);
                  }
                });
              }}
            >
              <h2>Review your stay</h2>
              <p>
                {guestId
                  ? staff?.guests.find((g) => g.id === guestId)?.data.name
                  : guest.name}{" "}
                ·{" "}
                {guestId
                  ? staff?.guests.find((g) => g.id === guestId)?.data.email
                  : guest.email}
              </p>
              <QuoteDetails quote={quote} />
              <label className={styles.check}>
                <input type="checkbox" required />I have reviewed the dates,
                full price and cancellation terms.
              </label>
              <div className={styles.actions}>
                <button
                  className={styles.secondary}
                  type="button"
                  disabled={busy}
                  onClick={() => setStep(1)}
                >
                  Back / refresh quote
                </button>
                <button className={styles.primary} disabled={busy}>
                  {busy ? "Confirming…" : "Confirm · pay at hotel"}
                </button>
              </div>
            </form>
          )}
        </>
      )}
    </div>
  );
}
