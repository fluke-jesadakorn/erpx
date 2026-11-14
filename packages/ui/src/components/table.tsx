"use client";
import {useContext,Children,cloneElement,isValidElement,type HTMLAttributes,type TableHTMLAttributes,type ReactNode} from "react";
import {RecordScope} from "@erpx/ui/components/record-context";
import { useUiNavigation, type TableActionElement } from "@erpx/ui/components/navigation-context";
import type { UiAppearance } from "@erpx/ui/components/appearance";
import { cn } from "@erpx/ui/lib/cn";

export function TableFrame({
  label,
  children,
  density = "comfortable",
  appearance = "legacy",
  className,
  ...props
}: HTMLAttributes<HTMLDivElement> & {
  label: string;
  children: ReactNode;
  density?: "compact" | "comfortable";
  appearance?: UiAppearance;
}) {
  return (
    <div
      data-slot="table-frame"
      data-density={density}
      data-appearance={appearance}
      role="region"
      aria-label={label}
      tabIndex={0}
      className={cn(
        "relative max-w-full overflow-auto rounded-[var(--ui-radius-lg)] border border-[var(--ui-line)] bg-[var(--ui-card)] focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-[var(--ui-brand)]",
        appearance === "standard" && "ui-standard-table",
        className,
      )}
      {...props}
    >
      {children}
    </div>
  );
}

type TableChild=TableActionElement;
type ActionCheck=(element:TableChild,base:string)=>boolean;
function rowRecordLink(node:ReactNode,base:string,isAction:ActionCheck):TableChild|null{
 for(const child of Children.toArray(node)){
  if(!isValidElement(child))continue;
  const element=child as TableChild;
  if(isAction(element,base))return element;
  const found=rowRecordLink(element.props.children,base,isAction);if(found)return found;
 }return null;
}
function recordRows(node:ReactNode,base:string,isAction:ActionCheck):boolean{return Children.toArray(node).some(child=>isValidElement(child)&&(child.type==='tbody'?!!rowRecordLink((child as TableChild).props.children,base,isAction):recordRows((child as TableChild).props.children,base,isAction)));}
function detailActions(node:ReactNode,base:string,isAction:ActionCheck,body=false,head=false):ReactNode{
 return Children.map(node,child=>{
  if(!isValidElement(child))return child;
  const element=child as TableChild;
  if(element.type==='tr'&&body){const link=rowRecordLink(element.props.children,base,isAction);return link?cloneElement(element,{},element.props.children,<td key="record-details"><span className="ui-record-open-action">{cloneElement(link,{},'View details')}</span></td>):cloneElement(element,{},Children.map(element.props.children,cell=>isValidElement(cell)&&(cell as TableChild).props.colSpan?cloneElement(cell as TableChild,{colSpan:(cell as TableChild).props.colSpan!+1}):cell),Children.toArray(element.props.children).some(cell=>isValidElement(cell)&&(cell as TableChild).props.colSpan)?null:<td key="record-details"/>);}
  if(element.type==='tr'&&head)return cloneElement(element,{},element.props.children,<th key="record-details"><span className="sr-only">Details</span></th>);
  return cloneElement(element,{},detailActions(element.props.children,base,isAction,body||element.type==='tbody'||element.type==='tfoot',head||element.type==='thead'));
 });
}

export function Table({ className, onClick, children, ...props }: TableHTMLAttributes<HTMLTableElement>) {
  const base=useContext(RecordScope),{isTableAction}=useUiNavigation();
  return (
    <table
      data-slot="table"
      className={cn("w-full border-collapse text-left text-sm", className)}
      {...props}
      onClick={event => {
        onClick?.(event);
        if (event.defaultPrevented || event.button !== 0 || event.metaKey || event.ctrlKey || event.shiftKey || event.altKey || window.getSelection()?.toString()) return;
        const target = event.target;
        if (!(target instanceof Element) || target.closest('a,button,input,select,textarea,label,summary,[contenteditable="true"]')) return;
        const row = target.closest('tbody tr');
        const link = row?.querySelector<HTMLElement>('[data-record-link="true"]');
        if (link) link.click();
      }}
    >{recordRows(children,base,isTableAction)?detailActions(children,base,isTableAction):children}</table>
  );
}
