import type {WorkCommandCompletion} from '@/lib/work_projection/work-fetch';
import {announceWorkChange} from '@/lib/work_projection/work-refresh';
export function captureLegalCommandCompletion():WorkCommandCompletion|undefined{const form=typeof document==='undefined'?null:document.activeElement?.closest<HTMLFormElement>('form');return form?{form,formEditVersion:form.getAttribute('data-erpx-edit-version')}:undefined;}
/** Only a parsed owner receipt can publish a saved-form event. The original
 * revision lets each drawer retain edits made while the command was pending. */
export function acknowledgeLegalCommand(completion?:WorkCommandCompletion){if(typeof window==='undefined')return;if(completion)window.dispatchEvent(new CustomEvent('erpx:record-command-saved',{detail:completion}));announceWorkChange();}
