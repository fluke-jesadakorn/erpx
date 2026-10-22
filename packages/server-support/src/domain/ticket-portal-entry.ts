export const supportTicketPortalLanding = "customers-sales/customers/service/portal";
/** Route identity only. Current ticket/binding authority stays in its owner. */
export function supportTicketPortalEntry(path: string): {bindingId?: string} | null {
  if (path === supportTicketPortalLanding) return {};
  if (!path.startsWith(supportTicketPortalLanding + "/")) return null;
  let bindingId: string;
  try { bindingId = decodeURIComponent(path.slice(supportTicketPortalLanding.length + 1)); } catch { return null; }
  if (!bindingId || bindingId.length > 200 || [".", "..", "new"].includes(bindingId) || /[\\/?#%\u0000-\u001f\u007f]/.test(bindingId)) return null;
  return {bindingId};
}
