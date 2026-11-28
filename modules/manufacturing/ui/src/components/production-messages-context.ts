"use client";
import {createContext} from "react";
export const ProductionMessages=createContext<{error:string;notice:string;uncertain:boolean;busy:boolean;retry:()=>Promise<void>;refreshSaved?:()=>void}|null>(null);
