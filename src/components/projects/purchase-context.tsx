"use client";
import {createContext,useContext,type ReactNode} from "react";
import type {ProjectPurchaseControls} from "@/lib/projects/purchase-controls";
import {ProjectEditorBoundary} from "./project-editor-boundary";
import type {ProjectEditorScope} from "@/lib/projects/project-editor-command";
const ProjectPurchases=createContext<ProjectPurchaseControls|undefined>(undefined);
export function ProjectPurchaseProvider({purchases,children,editorSource}:{purchases?:ProjectPurchaseControls;children:ReactNode;editorSource?:ProjectEditorScope}){return <ProjectPurchases.Provider value={purchases}>{editorSource?<ProjectEditorBoundary source={editorSource}>{children}</ProjectEditorBoundary>:children}</ProjectPurchases.Provider>;}
export function useProjectPurchases(){return useContext(ProjectPurchases);}
