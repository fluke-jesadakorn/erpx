"use client";
import dynamic from "next/dynamic";
export const PosRecordConsole = dynamic(() => import("../pos-record-console").then(module => module.PosRecordConsole));
