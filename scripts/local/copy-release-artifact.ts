import {cp} from 'node:fs/promises';
import {constants} from 'node:fs';

/** Copy a verified standalone release into its isolated execution directory. */
export async function copyReleaseArtifact(source:string,destination:string){
 await cp(source,destination,{recursive:true,mode:constants.COPYFILE_FICLONE,verbatimSymlinks:true});
}
