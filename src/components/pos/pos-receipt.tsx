"use client";
import {useFrontLanguage} from "@/components/front/front-language";
import {PosReceiptView} from "./pos-receipt-view";
import type {PosReceiptSnapshot} from "@/server/modules/pos/public/pos-daily";
export function PosReceipt({receipt}:{receipt:PosReceiptSnapshot|null}){return <PosReceiptView receipt={receipt} language={useFrontLanguage()}/>;}
