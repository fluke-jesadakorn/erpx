import { readFileSync, readdirSync } from 'node:fs';
import { join } from 'node:path';
import ts from 'typescript';
import { expect, it } from 'vitest';
import { widgetDefinitions, pageLayoutSchema, isConfigurablePage } from '../src/lib/page-layouts';
const files=(dir:string):string[]=>readdirSync(dir,{withFileTypes:true}).flatMap(e=>e.isDirectory()?files(join(dir,e.name)):e.name.endsWith('.tsx')?[join(dir,e.name)]:[]);
it('keeps widget identities unique, validates layouts, and registers declared page types',()=>{
 expect(widgetDefinitions.every(w=>isConfigurablePage(w.page))).toBe(true);
 const keys=widgetDefinitions.map(w=>`${w.page}:${w.id}`);expect(new Set(keys).size).toBe(keys.length);
 expect(pageLayoutSchema.safeParse({schemaVersion:1,widgets:[{id:'x',columns:12,height:null,hidden:false},{id:'x',columns:6,height:null,hidden:false}]}).success).toBe(false);
 for(const file of files('src')){
  const source=readFileSync(file,'utf8');if(!source.includes('features/feature-page'))continue;
  const tree=ts.createSourceFile(file,source,ts.ScriptTarget.Latest,true,ts.ScriptKind.TSX);
  const walk=(node:ts.Node)=>{
   if(ts.isJsxOpeningElement(node)||ts.isJsxSelfClosingElement(node)){
    const tag=node.tagName.getText(tree);
    if(['WidgetGrid','FeatureWidget'].includes(tag)){
     const attr=node.attributes.properties.find(a=>ts.isJsxAttribute(a)&&a.name.getText(tree)===(tag==='WidgetGrid'?'pageKey':'id')) as ts.JsxAttribute|undefined;
     if(attr?.initializer&&ts.isStringLiteral(attr.initializer)){
      const id=attr.initializer.text;
      if(tag==='WidgetGrid' && isConfigurablePage(id)) expect(widgetDefinitions.some(w=>w.page===id),`${file}: ${tag} ${id}`).toBe(true);
     }
    }
    if(['FeatureTabs','FeatureTabBar'].includes(tag)){
     let parent:ts.Node|undefined=node.parent;
     while(parent){if(ts.isJsxElement(parent))expect(parent.openingElement.tagName.getText(tree),`${file}: nested feature navigation`).not.toBe('FeatureWidget');parent=parent.parent;}
    }
   }
   ts.forEachChild(node,walk);
  };walk(tree);
 }
});
it('does not reintroduce the removed topic switchers',()=>{
 for(const [file,labels] of [
  ['src/app/holdings/[holdingSlug]/people/page.tsx',['People history']],
  ['src/app/holdings/[holdingSlug]/people/[id]/page.tsx',['Person sections']],
  ['src/components/calendar/calendar-workspace.tsx',['Calendar scope']],
  ['src/components/pos/pos-console.tsx',['role="tablist"']],
  ['src/components/executive/canvas.tsx',['className={styles.canvasTabs}']],
 ] as const){const source=readFileSync(file,'utf8');for(const label of labels)expect(source,file).not.toContain(label);}
});
