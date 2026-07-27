export class AuthoringController {
  #renderer; #getState; #revision=0; #nextRevision=1; #dirty=true; #applying=null; #listeners=new Set();
  constructor({renderer,getState}) { this.#renderer=renderer; this.#getState=getState; }
  get revision(){return this.#revision} get dirty(){return this.#dirty} get applying(){return !!this.#applying}
  subscribe(fn){this.#listeners.add(fn);return()=>this.#listeners.delete(fn)}
  markDirty(){this.#dirty=true;this.#emit()}
  #emit(){for(const fn of this.#listeners)fn({revision:this.#revision,dirty:this.#dirty,applying:!!this.#applying})}
  apply(adapt){
    if(this.#applying)return this.#applying;
    const revision=this.#nextRevision++; this.#dirty=false; this.#emit();
    this.#applying=(async()=>{try{const snapshot=await this.#getState();const ir=adapt(snapshot,revision);const compiled=await this.#renderer.compileGraph(ir);await this.#renderer.switchCompiledGraph(compiled.compiledId);this.#revision=revision;return compiled} catch(e){this.#dirty=true;throw e} finally{this.#applying=null;this.#emit()}})();
    return this.#applying;
  }
}
