const viewport = (canvas, ownerWindow) => ({
  width: Math.max(1, canvas.clientWidth),
  height: Math.max(1, canvas.clientHeight),
  dpr: Math.min(4, Math.max(1, ownerWindow.devicePixelRatio || 1)),
});
const sameViewport = (a, b) => a.width === b.width && a.height === b.height && a.dpr === b.dpr;
const sizeCanvas = (canvas, value) => {
  canvas.width = Math.round(value.width * value.dpr);
  canvas.height = Math.round(value.height * value.dpr);
};
const mods = e => ({ alt:e.altKey, control:e.ctrlKey, meta:e.metaKey, shift:e.shiftKey });

export function prepareBrowserHost(canvas, { onError=console.error, chooseNodeType }={}) {
  const ownerDocument=canvas.ownerDocument, ownerWindow=ownerDocument.defaultView ?? window;
  const originalTabIndex=canvas.getAttribute("tabindex"), originalTouchAction=canvas.style.touchAction;
  let view, dead=false, generation=0, resizing=false, pending, appliedViewport, menuPending=false, unsubscribeHost=()=>{};
  const captured=new Set();
  const initialViewport=viewport(canvas,ownerWindow); appliedViewport=initialViewport; sizeCanvas(canvas,initialViewport);
  canvas.tabIndex=0; canvas.style.touchAction="none";
  const point=e=>{const r=canvas.getBoundingClientRect();return{x:e.clientX-r.left,y:e.clientY-r.top}};
  const input=e=>{
    if(!view)return;
    if(e instanceof ownerWindow.PointerEvent){
      const phase=e.type==="pointerdown"?"down":e.type==="pointermove"?"move":e.type==="pointerup"?"up":"cancel";
      if(phase==="down"){menuPending=e.button===2&&!e.ctrlKey&&(e.buttons&1)===0;canvas.focus();try{canvas.setPointerCapture(e.pointerId);captured.add(e.pointerId)}catch{}}
      if((phase==="up"||phase==="cancel")&&captured.delete(e.pointerId))try{if(canvas.hasPointerCapture(e.pointerId))canvas.releasePointerCapture(e.pointerId)}catch{}
      view.feedInput({kind:"pointer",phase,pointerId:e.pointerId,pointerType:e.pointerType,position:point(e),button:e.button,buttons:e.buttons,modifiers:mods(e)});
    }else if(e instanceof ownerWindow.WheelEvent){
      e.preventDefault(); menuPending=false;
      const scale=e.deltaMode===ownerWindow.WheelEvent.DOM_DELTA_LINE?16:e.deltaMode===ownerWindow.WheelEvent.DOM_DELTA_PAGE?Math.max(1,canvas.clientHeight):1;
      view.feedInput({kind:"wheel",position:point(e),delta:{x:e.deltaX*scale,y:e.deltaY*scale},modifiers:mods(e)});
    }else if(e instanceof ownerWindow.KeyboardEvent){menuPending=false;view.feedInput({kind:"key",phase:e.type==="keydown"?"down":"up",key:e.key,code:e.code,repeat:e.repeat,modifiers:mods(e)});
    }else view.feedInput({kind:"focus",phase:e.type==="focus"?"focus":"blur"});
  };
  const names=["pointerdown","pointermove","pointerup","pointercancel","wheel","keydown","keyup","focus","blur"];
  const pump=()=>{
    if(!view||resizing||!pending||dead)return;
    const next=pending, currentGeneration=generation;pending=undefined;
    if(sameViewport(next,appliedViewport)){sizeCanvas(canvas,next);pump();return}
    resizing=true;
    Promise.resolve(view.setViewport(next)).then(()=>{if(dead||currentGeneration!==generation)return;appliedViewport=next;sizeCanvas(canvas,next)}).catch(error=>{if(!dead&&currentGeneration===generation)onError(error)}).finally(()=>{if(dead||currentGeneration!==generation)return;resizing=false;pump()});
  };
  const resize=()=>{if(dead)return;pending=viewport(canvas,ownerWindow);pump()};
  const outside=e=>{if(view&&e.button===0&&e.target!==canvas&&!canvas.contains(e.target)&&view.getHostSnapshot().colorPickerOpen)view.feedInput({kind:"outside-pointer",button:0})};
  const lost=e=>captured.delete(e.pointerId);
  const observer=new ownerWindow.ResizeObserver(resize);
  return {initialViewport,attach(_root,next){
    view=next;
    for(const n of names)canvas.addEventListener(n,input,{passive:n!=="wheel"});
    canvas.addEventListener("contextmenu",prevent);canvas.addEventListener("lostpointercapture",lost);
    ownerDocument.addEventListener("pointerdown",outside,true);ownerWindow.addEventListener("resize",resize);
    unsubscribeHost=view.onHostRequests(request=>{if(request.kind!=="add-node-menu"||!menuPending)return;menuPending=false;const typeId=chooseNodeType?.(request);if(typeId)view.addNode({typeId,viewPosition:request.viewPosition}).catch(onError)});
    observer.observe(canvas);resize();
  },destroy(){
    if(dead)return;dead=true;generation++;pending=undefined;observer.disconnect();unsubscribeHost();ownerWindow.removeEventListener("resize",resize);ownerDocument.removeEventListener("pointerdown",outside,true);
    for(const n of names)canvas.removeEventListener(n,input);canvas.removeEventListener("contextmenu",prevent);canvas.removeEventListener("lostpointercapture",lost);
    for(const id of captured)try{if(canvas.hasPointerCapture(id))canvas.releasePointerCapture(id)}catch{}captured.clear();
    if(originalTabIndex===null)canvas.removeAttribute("tabindex");else canvas.setAttribute("tabindex",originalTabIndex);canvas.style.touchAction=originalTouchAction;view=null;
  }};
}
function prevent(e){e.preventDefault()}
