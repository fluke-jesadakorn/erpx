"use client";
import {useProductNavigationScope} from "@erpx/workplace/components/product-link-context";
import {productNavigationHref,productHrefString} from "@erpx/feature-work_projection/lib/product-navigation";
import NextLink from 'next/link';
import type {ComponentProps} from 'react';
import {canonicalUiHref} from '@erpx/feature-work_projection/lib/destination-registry';
/** Every rendered link uses the same destination catalog as server projections. */
export default function WorkspaceLink({href,prefetch=false,...props}:ComponentProps<typeof NextLink>) {
 const scope=useProductNavigationScope(),value=productHrefString(href),destination=value===null?null:productNavigationHref(value,scope);
 const targetHref=typeof href==='string'?destination?.href??canonicalUiHref(href)
  :!scope.audience||!destination||!destination.handoff&&destination.href===value?href:destination.href;
 return <NextLink {...props} {...(destination?.handoff?{"data-product-switch":true}:{})} prefetch={destination?.handoff?false:prefetch} href={targetHref}/>;
}
