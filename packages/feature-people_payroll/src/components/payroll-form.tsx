"use client";
import { createContext, useContext, type ComponentPropsWithRef } from "react";
export const DraftScope = createContext<string | undefined>(undefined);

export function PayrollForm({onInputCapture, onChangeCapture, ...props}: ComponentPropsWithRef<"form">) {
  const scope = useContext(DraftScope) ?? "payroll-editor";
  const edited = (event: React.SyntheticEvent<HTMLFormElement>) => event.currentTarget.setAttribute("data-erpx-edit-version", crypto.randomUUID());
  return <form {...props} data-slot="form-layout" data-layout="grid" data-payroll-draft={scope} onInputCapture={event => {edited(event); onInputCapture?.(event);}} onChangeCapture={event => {edited(event); onChangeCapture?.(event);}} />;
}

