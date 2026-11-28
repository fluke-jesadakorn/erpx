"use client";
import {useFrontLanguage} from "@erpx/platform-front/components/front-language";
import {PosReceiptView} from "@erpx/pos-ui/components/pos-receipt-view";
import type {PosReceiptSnapshot} from "@erpx/pos-server/public/pos-daily";
export function PosReceipt({receipt}:{receipt:PosReceiptSnapshot|null}){return <PosReceiptView receipt={receipt} language={useFrontLanguage()}/>;}
