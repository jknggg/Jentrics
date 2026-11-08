import type { AgentId, Bridge, DashboardState, WakeOffer } from "./types.ts";
import { renderWakeDialog, snapshotWakeReason, WAKE_PROMPT, wakeOutcomeText, wakeReason } from "./wake-ui.ts";
import type { WakeViewState } from "./wake-ui.ts";

export function createWakeController(root:HTMLElement,content:HTMLElement,bridge:Bridge,getState:()=>DashboardState,setState:(state:DashboardState)=>void,paint:()=>void) {
    const view:WakeViewState={available:!!bridge.prepareWake && !!bridge.confirmWake,busy:{},messages:{}};
    let disposed=false;let dialog:HTMLElement|null=null;let awaiting:AgentId|null=null;let preparing=false;
    function close(id:AgentId){
        dialog?.remove();dialog=null;awaiting=null;content.removeAttribute("inert");
        if(!disposed)root.querySelector<HTMLButtonElement>(`#wake-${id}`)?.focus();
    }
    async function confirm(offer:WakeOffer){
        const id=offer.agentId;if(disposed || awaiting!==id || !offer.confirmationId || !bridge.confirmWake)return;
        close(id);view.busy[id]=true;paint();
        try{const result=await bridge.confirmWake(id,offer.confirmationId,true);if(disposed)return;view.messages[id]=wakeOutcomeText(result);setState(result.dashboard);}
        catch{if(!disposed)view.messages[id]="The request result could not be received; completion is unknown. No automatic retry was sent.";}
        finally{view.busy[id]=false;if(!disposed){paint();root.querySelector<HTMLButtonElement>(`#wake-${id}`)?.focus();}}
    }
    async function prepare(id:AgentId){
        if(disposed || preparing || view.busy[id] || awaiting || !bridge.prepareWake || !bridge.confirmWake || snapshotWakeReason(id,getState()))return;
        preparing=true;view.busy[id]=true;delete view.messages[id];paint();
        try{
            const offer=await bridge.prepareWake(id);if(disposed)return;
            if(!offer.enabled){view.messages[id]=wakeReason(offer.reason);return;}
            if(offer.agentId!==id || !offer.confirmationId || offer.prompt!==WAKE_PROMPT){view.messages[id]="Wake up is unavailable because its confirmation could not be verified.";return;}
            awaiting=id;
            dialog=renderWakeDialog(offer,bridge.preview,()=>{void confirm(offer);},()=>{if(awaiting!==id)return;view.messages[id]="Wake cancelled. No request was sent.";close(id);paint();root.querySelector<HTMLButtonElement>(`#wake-${id}`)?.focus();});
            root.append(dialog);content.setAttribute("inert","");dialog.querySelector<HTMLButtonElement>("#wake-cancel")?.focus();
        }catch{if(!disposed)view.messages[id]="Could not check whether Wake up is available. Try again.";}
        finally{preparing=false;view.busy[id]=false;if(!disposed){paint();if(!dialog)root.querySelector<HTMLButtonElement>(`#wake-${id}`)?.focus();}}
    }
    return {view,bind(){for(const id of ["codex","claude"] as const)root.querySelector<HTMLButtonElement>(`#wake-${id}`)?.addEventListener("click",()=>{void prepare(id);});},dispose(){disposed=true;dialog?.remove();dialog=null;awaiting=null;content.removeAttribute("inert");}};
}
