/** The only runtime adapter for terminology stored before Guest Accounts. */
const keys: Record<string, string> = {
  folio: "guestAccount", folios: "guestAccounts", folioId: "guestAccountId",
  toFolioId: "toGuestAccountId", folioIds: "guestAccountIds", folio_id: "guest_account_id",
  folio_entry_id: "guest_account_entry_id", folioEntryId: "guestAccountEntryId",
};
function command(value: string, legacy: boolean) {
  return legacy
    ? value.replace(/^((?:pms\.)?)guest_account\./, "$1folio.").replace("stay.route_guest_account", "stay.route_folio")
    : value.replace(/^((?:pms\.)?)folio\./, "$1guest_account.").replace("stay.route_folio", "stay.route_guest_account");
}
export function guestAccountOperation(value: string) { return command(value, false); }
export function legacyGuestAccountOperation(value: string) { return command(value, true); }
function convert(value: unknown, legacy: boolean): unknown {
  if (Array.isArray(value)) return value.map(item => convert(item, legacy));
  if (!value || typeof value !== "object") return value;
  const mapping = legacy ? Object.fromEntries(Object.entries(keys).map(([a, b]) => [b, a])) : keys;
  const output: Record<string, unknown> = {};
  for (const [key, item] of Object.entries(value)) {
    const name = mapping[key] ?? key;
    const next = ["operation","eventType"].includes(key) && typeof item === "string" ? command(item, legacy)
      : ["kind","recordKind"].includes(key) && item === (legacy ? "guest_account" : "folio") ? (legacy ? "folio" : "guest_account")
      : key === "section" && item === (legacy ? "guest-accounts" : "folios") ? (legacy ? "folios" : "guest-accounts")
      : convert(item, legacy);
    if (name in output && JSON.stringify(output[name]) !== JSON.stringify(next))
      throw new Error("GUEST_ACCOUNT_REFERENCE_CONFLICT");
    output[name] = next;
  }
  return output;
}
export function canonicalGuestAccount<T>(value: T): T { return convert(value, false) as T; }
export function legacyGuestAccount<T>(value: T): T { return convert(value, true) as T; }
export function parseGuestAccountJson<T = ReturnType<typeof JSON.parse>>(value: string): T { return canonicalGuestAccount(JSON.parse(value)) as T; }
/** Redirect links while preserving every selector and the selected record ID. */
export function guestAccountUrl(value: string, frontPms = false) {
  if (frontPms) value = value.replace(/\/folios(?=[/?#]|$)/g, "/guest-accounts");
  return value.replace(/\/pms\/folios(?=[/?#]|$)/g, "/pms/guest-accounts")
    .replace(/([?&])folio=/g, "$1guestAccount=");
}

/** Project immutable catalogue evidence into the current vocabulary. */
export function guestAccountCapabilityId(value: string) { return value === "pms/folios" ? "pms/guest-accounts" : value; }
