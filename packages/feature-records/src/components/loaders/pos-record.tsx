"use client";
import dynamic from "next/dynamic";
export const PosRecordConsole = dynamic(() => import("@erpx/feature-records/components/pos-record-console").then(module => module.PosRecordConsole));
