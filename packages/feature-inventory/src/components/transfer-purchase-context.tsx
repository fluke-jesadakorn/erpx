"use client";
import {createContext,useContext,type ReactNode} from "react";
import type {TransferPurchaseControls} from "@erpx/feature-inventory/lib/transfer-purchase-controls";
import {TransferDraftBoundary} from "@erpx/feature-inventory/components/transfer-draft-boundary";
const Purchase=createContext<TransferPurchaseControls|undefined>(undefined);
export const useTransferPurchase=()=>useContext(Purchase);
export function TransferEnvironment({purchase,children}:{purchase?:TransferPurchaseControls;children?:ReactNode}) {
  return <Purchase.Provider value={purchase}><TransferDraftBoundary>{purchase && !purchase.newWork && <p role="status">{purchase.settleExisting ? "New transfer plans, submission and first dispatch are paused. Existing warehouse settlement and history remain available." : "This transfer is available for retained history."}</p>}{children}</TransferDraftBoundary></Purchase.Provider>;
}
export function TransferPurchaseProvider({purchase,children}:{purchase?:TransferPurchaseControls;children?:ReactNode}){return <Purchase.Provider value={purchase}>{children}</Purchase.Provider>;}
