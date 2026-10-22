'use client';
import type {ReactNode} from 'react';
import {confirmContextSwitch,setContextSwitching} from '@erpx/feature-work_projection/lib/context-switch';
/** Native same-origin form; its server handler repeats session/Company/CAS. */
export function AdminRoleChoiceForm({children}:{children:ReactNode}){return <form method="post" action="/admin/launch" onSubmit={event=>{if(!confirmContextSwitch())event.preventDefault();else setContextSwitching(true);}}>{children}</form>;}
