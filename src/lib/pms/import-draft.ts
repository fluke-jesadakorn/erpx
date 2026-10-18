import type {PmsImportAction} from './import-command';

/** Mounted Import buffers only. No storage, authority or reload receipt. */
export function createPmsImportDraft() {
  const body={value:'',revision:0,accepted:'',acceptedRevision:0};
  const journal={value:'',revision:0,accepted:'',acceptedRevision:0};
  function edit(field:typeof body,value:string){
    if(field.value!==value){field.value=value;++field.revision;}
  }
  return {
    editBody(value:string){edit(body,value);},
    editJournal(value:string){edit(journal,value);},
    isDirty(){return [body,journal].some(field=>field.value!==field.accepted&&field.revision!==field.acceptedRevision);},
    capture(action:PmsImportAction){
      const field=action==='preview'?body:action==='approve'?journal:null;
      const value=field?.value,revision=field?.revision;
      return ()=>{
        if(field&&field.value===value&&field.revision===revision){
          field.accepted=field.value;field.acceptedRevision=field.revision;
        }
      };
    },
    discard(){
      for(const field of [body,journal]){
        edit(field,'');field.accepted='';field.acceptedRevision=field.revision;
      }
    },
  };
}
