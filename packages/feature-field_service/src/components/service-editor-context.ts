"use client";
import {createContext} from "react";
import type {useFieldServiceCommand} from "@erpx/feature-field_service/components/use-field-service-command";
export const ServiceEditorContext = createContext<(id: string, open: boolean) => void>(() => {});
export const ServiceCommandErrorContext = createContext<() => void>(() => {});
export const ServiceRecoveryContext=createContext<ReturnType<typeof useFieldServiceCommand>|null>(null);
