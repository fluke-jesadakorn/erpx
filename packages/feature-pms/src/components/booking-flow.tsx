"use client";
import { ActionAnchor as UiActionAnchor } from "@erpx/ui/components/foundation";

import {
  Button,
  Field,
  Select,
  Table,
  TableFrame,
  TextArea,
  TextInput,
} from "@erpx/ui/components/foundation";

import { PmsChoiceSelect } from "@erpx/feature-pms/components/choice-select";
import { BookingGallery } from "@erpx/feature-pms/components/booking-gallery";
import { useEffect, useRef, useState } from "react";
import { money, plusDays, type Guest } from "@erpx/server-pms/public/rules/pms-model";
import type { BookingQuote, RoomSelection } from "@erpx/server-pms/public/rules/pms-booking";
import { workFetch } from "@erpx/feature-work_projection/lib/work-fetch";
import { FrontLocalizedTree } from "@erpx/front/components/front-language";
import {BookingPolicySummary} from "@erpx/feature-pms/components/booking-policy-summary";
import styles from "@erpx/feature-pms/components/booking.module.css";
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
  images?: import("@erpx/server-pms/public/rules/pms-model").BookingImage[];
  roomContent: import("@erpx/server-pms/public/rules/pms-model").BookingSettings["roomContent"];
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
  if (!response.ok) throw new Error(data.error?.message ?? "Request failed. Please try again");
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
    <FrontLocalizedTree>
      <>
        <TableFrame label="Records" className={styles.scroll}>
          <Table className={styles.table}>
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
          </Table>
        </TableFrame>
        <p className={styles.total}>Stay total {money(quote.total, quote.currency)}</p>
        <div className={styles.paymentSplit}><p>Pay now <strong>{money(quote.dueNow,quote.currency)}</strong></p><p>Pay at hotel <strong>{money(quote.dueAtHotel,quote.currency)}</strong></p></div>
        {quote.lines.map((line,i)=><div key={i}><span>{line.roomName} · {line.rateName}</span><BookingPolicySummary policy={line.policy}/></div>)}
        {!confirmed && (
          <p className={styles.small}>
            Availability is checked again when you confirm. Searching does not hold a room.
          </p>
        )}
      </>
    </FrontLocalizedTree>
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
    [guestId, setGuestId] = useState(""),
    [selectedGuestLabel, setSelectedGuestLabel] = useState("");
  const [notes, setNotes] = useState(""),
    [source, setSource] = useState("direct"),
    [campaignCode, setCampaignCode] = useState(""),
    [externalRef, setExternalRef] = useState(""),
    [sourceName, setSourceName] = useState(""),
    [groupName, setGroupName] = useState(""),
    [blockId, setBlockId] = useState("");
  const [busy, setBusy] = useState(false),
    [error, setError] = useState(""),
    [confirmation, setConfirmation] = useState<{
      number?: string;
      state?: string;
      checkoutUrl?: string;
      attemptToken?: string;
      expiresAt?: number;
      refundState?: string | null;
      managementToken?: string;
      paymentState?: string;
    } | null>(null);
  const attempt = useRef<{ body: string; key: string } | null>(null);
  const base = `/api/public/booking/${encodeURIComponent(slug)}`;
  useEffect(() => {
    if (staff) return;
    const token = new URL(window.location.href).searchParams.get("attempt");
    if (!token) return;
    let active = true;
    const poll = async () => {
      try {
        const response = await fetch(`${base}/reservations?attempt=${encodeURIComponent(token)}`, {
          cache: "no-store",
        });
        const body = await response.json();
        if (!response.ok) throw new Error(body.error?.message ?? "Payment status unavailable");
        if (active) setConfirmation(body.data);
      } catch (error) {
        if (active) setError(error instanceof Error ? error.message : "Payment status unavailable");
      }
    };
    void poll();
    const timer = window.setInterval(() => void poll(), 5000);
    return () => {
      active = false;
      window.clearInterval(timer);
    };
  }, [base, staff]);
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
    <FrontLocalizedTree>
      <div className={styles.wrap}>
        <ol className={styles.steps}>
          {["1 · Find rooms", "2 · Guest details", "3 · Review & confirm"].map((text, i) => (
            <li key={text} aria-current={step === i ? "step" : undefined}>
              {text}
            </li>
          ))}
        </ol>
        {error && (
          <div className={styles.error} role="alert">
            {error}
          </div>
        )}
        {confirmation ? (
          <div className={styles.notice} role="status">
            <h2>
              {confirmation.state === "payment_pending"
                ? "Waiting for payment"
                : confirmation.state === "expired"
                  ? "Payment period expired"
                  : confirmation.state === "refund_review"
                    ? "Payment needs review"
                    : confirmation.state === "cancelled" ? "Booking cancelled" : confirmation.state === "confirmed" || staff ? "Booking confirmed" : "Checking booking status"}
              {confirmation.number ? ` · ${confirmation.number}` : ""}
            </h2>
            <p>
              {staff
                ? "The reservation is now available in the list below."
                : confirmation.state === "payment_pending"
                  ? "Your room is held for 30 minutes. Complete payment in Stripe Checkout."
                  : confirmation.state === "confirmed"
                    ? confirmation.paymentState === "received" ? "Payment confirmed. A confirmation email and private management link are on the way." : "Your room is reserved. Pay the balance at the hotel."
                    : confirmation.state === "refund_review"
                      ? "The hold expired before payment confirmation. Finance will review a refund."
                      : "The reservation is no longer held."}
            </p>
            {confirmation.state === "payment_pending" && confirmation.checkoutUrl && (
              <UiActionAnchor
                variant="secondary"
                className={styles.secondary}
                href={confirmation.checkoutUrl}
              >
                Continue to secure payment
              </UiActionAnchor>
            )}
            {confirmation.managementToken && <UiActionAnchor appearance="standard" variant="secondary" href={`/book/${slug}/manage#token=${encodeURIComponent(confirmation.managementToken)}`}>Manage booking</UiActionAnchor>}
            {confirmation.refundState && <p>Refund: {confirmation.refundState}</p>}
            {quote && <QuoteDetails quote={quote} confirmed />}
            <Button
              type="submit"
              variant="secondary"
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
            </Button>
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
                        staff?.propertyId ?? String(form.get("hotel") ?? propertyId ?? "");
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
                      <Field>
                        Hotel
                        <Select name="hotel">
                          <option value="">All hotels</option>
                          {hotels.map((h) => (
                            <option key={h.id} value={h.id}>
                              {h.name}
                            </option>
                          ))}
                        </Select>
                      </Field>
                    )}
                    {staff && (
                      <Field>
                        Group allocation
                        <PmsChoiceSelect api={`/api/v1${staff.base}/pms`} propertyId={staff.propertyId} kind="block" label="Group allocation" options={staff.blocks} value={blockId} onChange={setBlockId} emptyLabel="None" />
                      </Field>
                    )}
                    <Field>
                      Check-in
                      <TextInput
                        name="arrival"
                        type="date"
                        min={today}
                        defaultValue={today}
                        required
                      />
                    </Field>
                    <Field>
                      Check-out
                      <TextInput
                        name="departure"
                        type="date"
                        min={today}
                        defaultValue={plusDays(today, 1)}
                        required
                      />
                    </Field>
                    <Field>
                      Adults per room
                      <TextInput
                        name="adults"
                        type="number"
                        min={1}
                        max={50}
                        defaultValue={2}
                        required
                      />
                    </Field>
                    <Field>
                      Child ages per room
                      <TextInput
                        name="children"
                        placeholder="e.g. 4, 9"
                        aria-describedby="child-help"
                      />
                    </Field>
                    <Field>
                      Rooms per selection
                      <TextInput
                        name="quantity"
                        type="number"
                        min={1}
                        max={20}
                        defaultValue={1}
                        required
                      />
                    </Field>
                  </div>
                  <p id="child-help" className={styles.small}>
                    Leave child ages blank for adults only. Every guest counts toward room capacity.
                  </p>
                  <Button
                    clientManaged type="submit"
                    variant="primary"
                    className={styles.primary}
                    disabled={busy}
                  >
                    Search availability
                  </Button>
                </form>
                {results && (
                  <div className={styles.surface}>
                    <h2>Choose your rooms</h2>
                    {!results.some((r) => r.offers.length) && (
                      <p>
                        No rooms match these dates and guests. Try different dates or contact the
                        hotel.
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
                            <div className={styles.offer} key={`${result.hotel.id}-${line.rateId}`}>
                              <div>
                                <BookingGallery slug={slug} compact images={result.hotel.roomContent.find(c=>c.typeId===line.typeId)?.images}/>
                                <h3>{line.roomName}</h3>
                                <p className={styles.small}>{result.hotel.roomContent.find(c=>c.typeId===line.typeId)?.beds} · {result.hotel.roomContent.find(c=>c.typeId===line.typeId)?.sizeSqm ? `${result.hotel.roomContent.find(c=>c.typeId===line.typeId)?.sizeSqm} m²` : ""} · {result.hotel.roomContent.find(c=>c.typeId===line.typeId)?.amenities}</p>
                                <p>
                                  {line.rateName} · {line.quantity} room(s) · {line.nights.length}{" "}
                                  night(s)
                                </p>
                                <p>
                                  {
                                    result.hotel.roomContent.find((c) => c.typeId === line.typeId)
                                      ?.description
                                  }
                                </p>
                                <p>{line.inclusions}</p>
                                <p className={styles.small}>{q.policy?.text}</p>
                              </div>
                              <div>
                                <div className={styles.price}>{money(q.total, q.currency)}</div>
                                <p className={styles.small}>Entire stay, including tax & service</p><p>Pay now {money(q.dueNow,q.currency)}</p><p>Pay at hotel {money(q.dueAtHotel,q.currency)}</p>
                                <Button
                                  variant="secondary"
                                  type="button"
                                  className={styles.secondary}
                                  disabled={busy}
                                  onClick={() => {
                                    if (rooms.length && selectedHotel !== result.hotel.id) {
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
                                </Button>
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
                              .find((q) => q.lines[0].rateId === r.rateId)?.lines[0].roomName
                          }{" "}
                          · {r.quantity} room(s) · {r.arrival} → {r.departure}
                        </span>
                        <Button
                          type="submit"
                          variant="secondary"
                          className={styles.secondary}
                          onClick={() => setRooms(rooms.filter((_, index) => index !== i))}
                        >
                          Remove
                        </Button>
                      </div>
                    ))}
                    <Button
                      type="submit"
                      variant="primary"
                      className={styles.primary}
                      disabled={busy}
                      onClick={() => setStep(1)}
                    >
                      Continue to guest details
                    </Button>
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
                  <Field className={styles.field}>
                    Guest profile
                    <PmsChoiceSelect api={`/api/v1${staff.base}/pms`} propertyId={staff.propertyId} kind="guest" label="Guest profile" options={staff.guests.map(g=>({id:g.id,name:g.data.name+(g.data.email?" · "+g.data.email:"" )}))} value={guestId} onChange={(id,option)=>{setGuestId(id);setSelectedGuestLabel(option?.name??"");}} emptyLabel="Create a guest with this booking" />
                  </Field>
                )}
                {!guestId && (
                  <div className={styles.grid}>
                    <Field>
                      Full name
                      <TextInput
                        required
                        value={guest.name}
                        maxLength={200}
                        autoComplete="name"
                        onChange={(e) => setGuest({ ...guest, name: e.target.value })}
                      />
                    </Field>
                    <Field>
                      Email
                      <TextInput
                        required
                        type="email"
                        value={guest.email}
                        autoComplete="email"
                        onChange={(e) => setGuest({ ...guest, email: e.target.value })}
                      />
                    </Field>
                    <Field>
                      Phone
                      <TextInput
                        required
                        type="tel"
                        value={guest.phone}
                        maxLength={80}
                        autoComplete="tel"
                        onChange={(e) => setGuest({ ...guest, phone: e.target.value })}
                      />
                    </Field>
                  </div>
                )}
                <Field className={styles.field}>
                  Special requests
                  <TextArea
                    value={notes}
                    maxLength={2000}
                    onChange={(e) => setNotes(e.target.value)}
                    placeholder="Requests are subject to hotel confirmation"
                  />
                </Field>
                {staff && (
                  <div className={styles.grid}>
                    <Field>
                      Booking source
                      <Select value={source} onChange={(e) => setSource(e.target.value)}>
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
                      </Select>
                    </Field>
                    {source === "other_ota" && (
                      <Field>
                        OTA name
                        <TextInput
                          required
                          value={sourceName}
                          onChange={(e) => setSourceName(e.target.value)}
                        />
                      </Field>
                    )}
                    <Field>
                      Group name
                      <TextInput value={groupName} onChange={(e) => setGroupName(e.target.value)} />
                    </Field>
                    <Field>
                      External confirmation
                      <TextInput
                        value={externalRef}
                        onChange={(e) => setExternalRef(e.target.value)}
                      />
                    </Field>
                    <Field>
                      Campaign code recorded by staff
                      <TextInput
                        value={campaignCode}
                        onChange={(e) => setCampaignCode(e.target.value)}
                        maxLength={60}
                        placeholder="Optional campaign code"
                      />
                    </Field>
                    <Field>
                      Group allocation
                      <PmsChoiceSelect api={`/api/v1${staff.base}/pms`} propertyId={staff.propertyId} kind="block" label="Group allocation" options={staff.blocks} value={blockId} onChange={setBlockId} emptyLabel="None" />
                    </Field>
                  </div>
                )}
                <div className={styles.actions}>
                  <Button
                    variant="secondary"
                    type="button"
                    className={styles.secondary}
                    onClick={() => setStep(0)}
                  >
                    Back to rooms
                  </Button>
                  <Button
                    clientManaged type="submit"
                    variant="primary"
                    className={styles.primary}
                    disabled={busy}
                  >
                    Review price & policy
                  </Button>
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
                        source: source === "other_ota" ? sourceName.trim() : source,
                        groupName,
                        externalRef,
                        campaignCode,
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
                      const result = await jsonRequest(`${base}/reservations`, input, false, {
                        "Idempotency-Key": attempt.current.key,
                      });
                      setConfirmation(result);
                      if (result.checkoutUrl) window.location.assign(result.checkoutUrl);
                    }
                  });
                }}
              >
                <h2>Review your stay</h2>
                <p>
                  {guestId ? selectedGuestLabel || staff?.guests.find((g) => g.id === guestId)?.data.name : [guest.name, guest.email].filter(Boolean).join(" · ")}
                </p>
                <QuoteDetails quote={quote} />
                <Field orientation="inline" className={styles.check}>
                  <TextInput type="checkbox" required />I have reviewed the dates, full price and
                  cancellation terms.
                </Field>
                <div className={styles.actions}>
                  <Button
                    variant="secondary"
                    className={styles.secondary}
                    type="button"
                    disabled={busy}
                    onClick={() => setStep(1)}
                  >
                    Back / refresh quote
                  </Button>
                  <Button
                    clientManaged type="submit"
                    variant="primary"
                    className={styles.primary}
                    disabled={busy}
                  >
                    {busy ? "Preparing booking…" : quote.dueNow && !staff ? "Continue to secure payment" : "Confirm reservation"}
                  </Button>
                </div>
              </form>
            )}
          </>
        )}
      </div>
    </FrontLocalizedTree>
  );
}
