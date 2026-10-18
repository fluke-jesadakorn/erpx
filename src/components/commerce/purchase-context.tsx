"use client";
import {createContext, useContext, type ReactNode} from "react";
import type {CommercePurchaseControls} from "@/lib/commerce/purchase-controls";
import {CommerceDraftBoundary} from "./commerce-draft-boundary";
const Purchase = createContext<CommercePurchaseControls | undefined>(undefined);
export const useCommercePurchase = () => useContext(Purchase);
export function CommerceEnvironment({purchase, children}: {purchase?: CommercePurchaseControls; children?: ReactNode}) {
  return <Purchase.Provider value={purchase}><CommerceDraftBoundary>{purchase && !purchase.newWork && <p className="notice" role="status">{purchase.settleExisting
    ? "New document preparation is paused for this package. Existing obligations and history remain available." : "These documents are available for retained history."}</p>}{children}</CommerceDraftBoundary></Purchase.Provider>;
}
export function CommercePurchaseProvider({purchase, children}: {purchase?: CommercePurchaseControls; children?: ReactNode}) {
  return <Purchase.Provider value={purchase}>{children}</Purchase.Provider>;
}
