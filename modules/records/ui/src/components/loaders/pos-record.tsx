"use client";
import dynamic from "next/dynamic";
export const PosRecordConsole = dynamic(() => import("@erpx/records-ui/components/pos-record-console").then(module => module.PosRecordConsole));
