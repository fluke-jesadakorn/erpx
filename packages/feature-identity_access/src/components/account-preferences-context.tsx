"use client";
import { createContext, useContext } from "react";
import type { PreferenceState } from "@erpx/feature-identity_access/lib/account-preferences-client";

/** State transport only; Front consumers do not import the Account controller. */
export const AccountPreferencesContext = createContext<PreferenceState | null>(null);
export function useAccountPreferences() { return useContext(AccountPreferencesContext); }
