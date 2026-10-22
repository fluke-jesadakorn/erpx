export type GuestBookingIdentity = {id: string; number: string};
export type GuestBookingChangeScope = GuestBookingIdentity & {base: string; token: string; epoch: number};
export type GuestBookingChangeReceipt = GuestBookingIdentity & {state: string; checkoutUrl?: string | null; attemptToken?: string};
/** A native Checkout link belongs only to the still-pending payment attempt.
 * Fresh management history can settle/expire it before the confirmation arrives. */
export function guestBookingCheckoutContinuation(receipt: Pick<GuestBookingChangeReceipt, "checkoutUrl" | "attemptToken"> | null, booking: {pendingChange: {attemptToken: string} | null}) {
  return receipt?.attemptToken && receipt.attemptToken === booking.pendingChange?.attemptToken ? receipt.checkoutUrl || null : null;
}
type Kind = "change" | "cancel";
type Attempt = {
  scope: GuestBookingChangeScope; kind: Kind; body: string; key: string; refund: number;
  sent: boolean; receipt?: GuestBookingChangeReceipt; turn?: symbol;
};
const changed = () => new DOMException("Booking context changed. Return to the original booking before recovering this action.", "AbortError");
const image = (scope: GuestBookingChangeScope | null) => scope && JSON.stringify(scope);
const object = (value: unknown): value is Record<string, unknown> => !!value && typeof value === "object";

/** A signed proposal is one original owner command. A failed history refresh
 * does not turn an acknowledged confirmation into a second mutation. */
export function createGuestBookingChangeClient<Booking extends GuestBookingIdentity>(
  current: () => GuestBookingChangeScope | null = () => null,
  send: typeof fetch = fetch,
  newKey: () => string = () => crypto.randomUUID(),
) {
  let pending: Attempt | null = null, busy = false;
  const assertCurrent = (attempt: Attempt) => {
    if (image(current()) !== image(attempt.scope)) throw changed();
  };
  async function run(attempt: Attempt) {
    if (busy) throw Error("Wait for the current booking request.");
    assertCurrent(attempt);
    busy = true;
    const turn = Symbol("booking-change"); attempt.turn = turn;
    try {
      if (!attempt.receipt) {
        const first = !attempt.sent; attempt.sent = true;
        const response = await send(`${attempt.scope.base}/${attempt.kind}-confirm`, {
          method: "POST", cache: "no-store",
          headers: {"Content-Type": "application/json", Authorization: `Bearer ${attempt.scope.token}`, "Idempotency-Key": attempt.key},
          body: attempt.body,
        });
        const envelope: unknown = await response.json();
        assertCurrent(attempt);
        if (!response.ok) {
          const error = object(envelope) && object(envelope.error) ? envelope.error : null;
          // Native confirmation commits before opening Checkout. Even the first
          // 403/409 can therefore follow committed room/payment effects. Only
          // this native pre-effect, rolled-back stale-proposal rejection proves
          // that a fresh key did not commit; later failures remain recoverable.
          if (first && response.status === 409 && error?.code === "PMS_PROPOSAL_STALE") pending = null;
          throw Error(typeof error?.message === "string" ? error.message : "Unable to confirm the booking. Recover the original action before starting another.");
        }
        const data = object(envelope) && object(envelope.data) ? envelope.data : null;
        if (!data || data.id !== attempt.scope.id || data.number !== attempt.scope.number || typeof data.state !== "string"
          || data.checkoutUrl !== undefined && data.checkoutUrl !== null && typeof data.checkoutUrl !== "string"
          || data.attemptToken !== undefined && typeof data.attemptToken !== "string")
          throw Error("The booking confirmation is incomplete. Recover the original action.");
        attempt.receipt = data as GuestBookingChangeReceipt;
      }
      assertCurrent(attempt);
      const response = await send(`${attempt.scope.base}/manage`, {
        method: "POST", cache: "no-store", headers: {"Content-Type": "application/json", Authorization: `Bearer ${attempt.scope.token}`}, body: "{}",
      });
      const envelope: unknown = await response.json();
      assertCurrent(attempt);
      const data = object(envelope) && object(envelope.data) ? envelope.data : null;
      if (!response.ok || !data || data.id !== attempt.scope.id || data.number !== attempt.scope.number)
        throw Error("Your confirmation was received, but booking history could not refresh. Recover to refresh it; the confirmation will not be submitted again.");
      return {
        booking: data as Booking, receipt: attempt.receipt, refund: attempt.refund,
        acknowledge() {
          assertCurrent(attempt);
          if (pending !== attempt || attempt.turn !== turn) return false;
          pending = null; return true;
        },
      };
    } finally { busy = false; }
  }
  return {
    bind: (read: () => GuestBookingChangeScope | null) => {current = read;},
    hasPending: () => pending !== null,
    needsHistory: () => !!pending?.receipt,
    async confirm(kind: Kind, proposalToken: string, refund: number) {
      const scope = current(); if (!scope?.token) throw changed();
      const body = JSON.stringify({proposalToken});
      if (pending && (image(pending.scope) !== image(scope) || pending.kind !== kind || pending.body !== body))
        throw Error("Recover the original booking action before reviewing different changes.");
      if (!pending) pending = {scope: {...scope}, kind, body, key: newKey(), refund, sent: false};
      return run(pending);
    },
    async recover() {
      if (!pending) throw Error("There is no pending booking action.");
      return run(pending);
    },
  };
}
