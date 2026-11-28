"use client";

import type { ReactNode } from "react";
import { RecordScopeProvider as Scope } from "@erpx/platform-ui/components/record-context";
import { UiNavigationProvider, type TableActionElement } from "@erpx/platform-ui/components/navigation-context";
import { RecordLink } from "@erpx/records-ui/components/record-link";
import { CollectionRecordLink } from "@erpx/records-ui/components/collection-record-link";
import { recordTarget } from "@erpx/records-ui/lib/record-drawer";

const collectionKinds = new Set(['pms_entry','pms_hotel_sources','pms_hotel_record','iam_assignment','iam_group','usage_item','usage_group','billing_cost','billing_source','billing_statement','billing_line','platform_provision','platform_operation','platform_executive_sources']);

/** Record ownership decides which rendered row element opens its detail surface. */
export function isRecordTableAction(element: TableActionElement, base: string): boolean {
  return element.type === CollectionRecordLink || !!element.props.record ||
    !!(typeof element.props.href === "string" && base && recordTarget(element.props.href, base)) ||
    !!(typeof element.props.kind === "string" && element.props.id && collectionKinds.has(element.props.kind));
}

const bindings = { Link: RecordLink, isTableAction: isRecordTableAction };

export function RecordScopeProvider({ base, children }: { base: string; children: ReactNode }) {
  return <Scope base={base}><UiNavigationProvider bindings={bindings}>{children}</UiNavigationProvider></Scope>;
}
