"use client";
import {createContext,useContext,useState,cloneElement,isValidElement,type ReactNode} from 'react';
import type {ProjectEditorScope} from '@/lib/projects/project-editor-command';
const ProjectEditorContext=createContext<{source:ProjectEditorScope;active:(value:boolean)=>void}|null>(null);
/** Preserve only an already-active owner draft/request across eligibility or
 * affiliation changes. Initial absence and ordinary record refresh stay native. */
export function ProjectEditorBoundary({source,children}:{source:ProjectEditorScope;children:ReactNode}){
 const [active,setActive]=useState(false),[retained,setRetained]=useState({incoming:children,rendered:children});
 let rendered=retained.rendered;
 if(children&&(children!==retained.incoming||!active&&rendered!==children)){
  rendered=active&&isValidElement(children)&&isValidElement(retained.rendered)&&children.type===retained.rendered.type&&children.key!==retained.rendered.key?(retained.rendered.key===null?retained.rendered:cloneElement(children,{key:retained.rendered.key})):children;
  setRetained({incoming:children,rendered});
 }
 return <ProjectEditorContext value={{source,active:setActive}}>{children?rendered:active?retained.rendered:null}</ProjectEditorContext>;
}
export function useProjectEditorBoundary(){return useContext(ProjectEditorContext);}
