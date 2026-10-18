"use client";
import {createContext,useContext,type ReactNode} from "react";
import type {QualityPurchaseControls} from "@/lib/quality/quality-purchase-controls";
import {QualityDraftBoundary} from "./quality-draft-boundary";
const Context=createContext<{purchase?:QualityPurchaseControls;sources?:Record<string,QualityPurchaseControls>}>({});
export const useQualityPurchaseScope=()=>useContext(Context);
export function useQualityPurchase(id?:string){const context=useContext(Context);return id&&context.sources?.[id]?context.sources[id]:context.purchase;}
export function QualityEnvironment({purchase,sources,children}:{purchase?:QualityPurchaseControls;sources?:Record<string,QualityPurchaseControls>;children?:ReactNode}){return <Context.Provider value={{purchase,sources}}><QualityDraftBoundary>{purchase&&!purchase.newWork&&<p role="status">{"Advanced Quality setup and manual quarantine are unavailable. Existing sources retain their authorized settlement or history."}</p>}{children}</QualityDraftBoundary></Context.Provider>;}
export function QualityPurchaseProvider({purchase,children}:{purchase?:QualityPurchaseControls;children?:ReactNode}){return <Context.Provider value={{purchase}}>{children}</Context.Provider>;}
