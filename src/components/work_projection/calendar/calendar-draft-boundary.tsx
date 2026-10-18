'use client';
import {useEffect,useState,type ReactNode} from 'react';
import {setContextSwitching} from '@/lib/work_projection/context-switch';
import {installCalendarPendingNavigation} from '@/lib/work_projection/calendar-pending-navigation';
import {Dialog} from '@/components/ui/dialog';
import {Button} from '@/components/ui/foundation';
import type {UiAppearance} from '@/components/ui/appearance';
export function CalendarDraftBoundary({dirty,busy=false,pending=false,children,appearance="legacy"}:{dirty:boolean;busy?:boolean;pending?:boolean;children:ReactNode;appearance?:UiAppearance}){
 const [destination,setDestination]=useState<string|null>(null),held=dirty||busy||pending;
 useEffect(()=>installCalendarPendingNavigation(()=>held,setDestination),[held]);
 return <>{children}<Dialog appearance={appearance} open={destination!==null} title="Leave unsaved Calendar work?" onClose={()=>setDestination(null)}><p>{busy?'The submitted Calendar action is still waiting for its result. Keep this page open.':pending?'The original Calendar result has not been confirmed. Recover it in this account and role before leaving.':'Your Calendar draft has not been saved.'}</p><div className="ui-form-actions"><Button appearance={appearance} data-autofocus onClick={()=>setDestination(null)}>Keep editing</Button>{!busy&&!pending&&<Button appearance={appearance} variant="danger" onClick={()=>{if(destination){setContextSwitching(true);window.location.assign(destination);}}}>Discard and leave</Button>}</div></Dialog></>;
}
