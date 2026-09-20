"use client";
import type { Widget } from "@/lib/executive/canvas";
import { resizeGrid } from "@/lib/widget-geometry";
import { useRef,useState,type CSSProperties,type DragEvent,type PointerEvent,type ReactNode } from "react";
import styles from "./canvas.module.css";
export const widgetColumns=(w:Widget)=>w.columns??({third:4,half:6,full:12}[w.width]);
export const resizeDimensions = resizeGrid;
export function ResizableWidget({widget,editable,selected,onSelect,onResize,onDrop,children}:{widget:Widget;editable:boolean;selected:boolean;onSelect:()=>void;onResize:(size:{columns:number;height:number})=>void;onDrop:(event:DragEvent<HTMLElement>)=>void;children:ReactNode}) {
  const element=useRef<HTMLElement>(null),drag=useRef<{x:number;y:number;columns:number;height:number;gridWidth:number;edge:string}|null>(null);
  const [preview,setPreview]=useState<{columns:number;height:number}|null>(null);
  const size=preview??{columns:widgetColumns(widget),height:widget.height};
  function handlePointerDown(e:PointerEvent<HTMLButtonElement>){if(e.button!==0)return;e.preventDefault();e.stopPropagation();const box=element.current!.getBoundingClientRect();drag.current={x:e.clientX,y:e.clientY,columns:widgetColumns(widget),height:widget.height??Math.round(box.height),gridWidth:element.current!.parentElement!.getBoundingClientRect().width,edge:e.currentTarget.dataset.edge!};e.currentTarget.setPointerCapture(e.pointerId);onSelect();}
  function move(e:PointerEvent<HTMLButtonElement>){const d=drag.current;if(!d)return;setPreview(resizeDimensions(d.columns,d.height,e.clientX-d.x,e.clientY-d.y,d.gridWidth,d.edge));}
  function finish(e:PointerEvent<HTMLButtonElement>){const d=drag.current;if(!d)return;const next=resizeDimensions(d.columns,d.height,e.clientX-d.x,e.clientY-d.y,d.gridWidth,d.edge);drag.current=null;setPreview(null);if(next.columns!==d.columns||next.height!==d.height)onResize(next);if(e.currentTarget.hasPointerCapture(e.pointerId))e.currentTarget.releasePointerCapture(e.pointerId);}
  return <article ref={element} className={styles.widget} data-selected={selected} data-resizing={!!preview} style={{"--widget-columns":size.columns,"--widget-height":size.height?`${size.height}px`:undefined} as CSSProperties} onClick={onSelect} onDragOver={e=>e.preventDefault()} onDrop={onDrop}>
    <div className={styles.widgetBody}>{children}</div>
    {editable&&["left","right","bottom","corner"].map(edge=><button key={edge} className={styles.resizeHandle} data-edge={edge} aria-label={`Resize ${widget.title} ${edge} edge`} title="Drag to resize. Arrow keys also resize." onClick={e=>e.stopPropagation()} onPointerDown={handlePointerDown} onPointerMove={move} onPointerUp={finish} onPointerCancel={()=>{drag.current=null;setPreview(null);}} onKeyDown={e=>{if(!["ArrowLeft","ArrowRight","ArrowUp","ArrowDown"].includes(e.key))return;e.preventDefault();e.stopPropagation();const columns=widgetColumns(widget),height=widget.height??Math.round(element.current!.getBoundingClientRect().height);onResize({columns:Math.max(2,Math.min(12,columns+(e.key==="ArrowRight"?1:e.key==="ArrowLeft"?-1:0))),height:Math.max(240,Math.min(900,height+(e.key==="ArrowDown"?24:e.key==="ArrowUp"?-24:0)))});}}>{edge==="corner"?<span>◢</span>:null}</button>)}
    {preview&&<output className={styles.sizeIndicator}>{preview.columns}/12 · {preview.height}px</output>}
  </article>;
}
