"use client";
import {createContext,useContext,type ReactNode} from "react";
import type {CountPurchaseControls} from "@erpx/feature-inventory/lib/count-purchase-controls";
import {CountDraftBoundary} from "@erpx/feature-inventory/components/count-draft-boundary";
const Purchase=createContext<CountPurchaseControls|undefined>(undefined);
export const useCountPurchase=()=>useContext(Purchase);
export function CountEnvironment({purchase,children}:{purchase?:CountPurchaseControls;children?:ReactNode}) {
  return <Purchase.Provider value={purchase}><CountDraftBoundary>{purchase && !purchase.newWork && <p role="status">{purchase.settleExisting ? "New counts, recounts and submission are paused. Existing count decisions and history remain available." : "This count is available for retained history."}</p>}{children}</CountDraftBoundary></Purchase.Provider>;
}
export function CountPurchaseProvider({purchase,children}:{purchase?:CountPurchaseControls;children?:ReactNode}){return <Purchase.Provider value={purchase}>{children}</Purchase.Provider>;}
