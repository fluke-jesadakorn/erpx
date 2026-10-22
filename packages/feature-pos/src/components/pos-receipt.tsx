"use client";
import {useFrontLanguage} from "@erpx/front/components/front-language";
import {PosReceiptView} from "@erpx/feature-pos/components/pos-receipt-view";
import type {PosReceiptSnapshot} from "@erpx/server-pos/public/pos-daily";
export function PosReceipt({receipt}:{receipt:PosReceiptSnapshot|null}){return <PosReceiptView receipt={receipt} language={useFrontLanguage()}/>;}
