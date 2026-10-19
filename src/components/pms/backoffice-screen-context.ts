import type {BackofficePmsWorkspaceContext as OwnerContext} from "./standard-pms-backoffice-workspace";

/** Shared native widget inputs. The mounted standard owner supplies its guarded
 * form/send closures; these leaves never consume the parent's recovery state.
 * Each contract keeps only fields its real leaf reads, with their exact owner
 * types and requiredness. Both the retained original and current owner must
 * satisfy these structural props at their actual JSX callsites. */
export type BackofficePmsWorkspaceCashierScreenContext = Pick<OwnerContext, "form" | "rows" | "data" | "money">;
export type BackofficePmsWorkspaceDashboardScreenContext = Pick<OwnerContext, "stays" | "today" | "rows" | "tr" | "dailyPmsHref" | "p" | "guestName" | "roomName" | "s">;
export type BackofficePmsWorkspaceFrontDeskScreenContext = Pick<OwnerContext, "base" | "p" | "today" | "query" | "s" | "recordSelection" | "router" | "setQuery" | "stays" | "selectedRecord" | "guestName" | "roomName" | "can" | "data" | "rows" | "dailyPmsHref" | "form" | "rooms" | "guests" | "guestAccounts" | "rates">;
export type BackofficePmsWorkspaceGuestAccountsScreenContext = Pick<OwnerContext, "currentGuestAccount" | "setGuestAccountId" | "guestAccounts" | "guestName" | "money" | "rows" | "data" | "form" | "propertyId" | "erpBase" | "payments" | "base" | "p" | "openShifts" | "can" | "crossLinks" | "tr" | "frontMode" | "router" | "setError" | "setNotice">;
export type BackofficePmsWorkspaceGuestsScreenContext = Pick<OwnerContext, "form" | "query" | "setQuery" | "rows" | "tr" | "can" | "stays" | "roomName" | "dailyPmsHref" | "p">;
export type BackofficePmsWorkspaceInventoryScreenContext = Pick<OwnerContext, "crossLinks" | "erpBase" | "form" | "data" | "rows" | "s" | "selectedRecord" | "tr" | "rooms" | "guestAccounts" | "guests" | "today" | "roomName" | "base" | "can">;
export type BackofficePmsWorkspaceNightAuditScreenContext = Pick<OwnerContext, "tr" | "today" | "p" | "data" | "money" | "crossLinks" | "dailyPosHref" | "s" | "form" | "rows">;
export type BackofficePmsWorkspaceReportsScreenContext = Pick<OwnerContext, "crossLinks" | "erpBase" | "base" | "p" | "today" | "data" | "rows" | "money" | "dailyPmsHref">;
export type BackofficePmsWorkspaceReservationsScreenContext = Pick<OwnerContext, "from" | "setFrom" | "rows" | "s" | "can" | "p" | "base" | "today" | "send" | "form" | "roomTypes" | "query" | "setQuery" | "guestName" | "stays" | "roomName" | "money" | "dailyPmsHref" | "data">;
export type BackofficePmsWorkspaceRoomsHousekeepingScreenContext = Pick<OwnerContext, "section" | "rows" | "stays" | "form" | "today" | "selectedRecord" | "roomName" | "data" | "base" | "propertyId"> & Partial<Pick<OwnerContext, "taskCompletionDrafts" | "updateTaskCompletionDraft" | "discardTaskCompletionDraft">>;
export type BackofficePmsWorkspaceSettingsScreenContext = Pick<OwnerContext, "can" | "erpBase" | "s" | "send" | "data" | "p" | "base" | "rows" | "tr" | "form" | "today" | "roomTypes" | "rateFields" | "money">;
