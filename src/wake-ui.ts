import type { AgentId, DashboardState, WakeOffer, WakeOutcome, WakeReason } from "./types.ts";

export const WAKE_PROMPT="Reply only with hello.";
export interface WakeViewState { available:boolean; busy:Partial<Record<AgentId,boolean>>; messages:Partial<Record<AgentId,string>> }
const reasons:Record<WakeReason,string>={
    retryPolicyUnsupported:"This CLI cannot disable provider-managed retries. Wake up is unavailable under the single-request policy.",
    notInstalled:"Install a supported native CLI to wake this agent.",unsupported:"This CLI version does not support verified isolated wake requests.",signInRequired:"Sign in to the native CLI before waking this agent.",subscriptionRequired:"Wake up requires a first-party subscription sign-in.",activeWindow:"A five-hour window is already active. Wake up cannot reset it.",weeklyOnly:"This account reports weekly-only limits. Wake up cannot create a five-hour allowance.",busy:"A wake request is already in progress.",cooldown:"Wait 60 seconds after the last launch before trying again.",stopped:"The local monitor is shutting down.",confirmationRequired:"Confirm the request before sending it.",prepareRequired:"Check Wake up again before confirming.",expiredConfirmation:"This confirmation expired. Check Wake up again.",providerUnavailable:"The provider is unavailable for a wake request.",timedOut:"Timed out · completion unknown. No automatic retry was sent.",outputLimit:"The request exceeded its response limit. Completion is unknown.",requestFailed:"The wake request failed.",cancelled:"Wake request cancelled."
};
export function wakeReason(reason:WakeReason|null):string {return reason?reasons[reason] ?? "Wake up is unavailable.":"Wake up is unavailable.";}
function node<K extends keyof HTMLElementTagNameMap>(tag:K,text="",className=""):HTMLElementTagNameMap[K] {
    const result=document.createElement(tag);result.textContent=text;if(className)result.className=className;return result;
}
export function snapshotWakeReason(id:AgentId,state:DashboardState,transportError=false):WakeReason|null {
    const agent=state.agents.find(agent=>agent.agentId===id);const now=Date.now();
    const fresh=(observed:number)=>observed>0 && now>=observed && now-observed<900_000;
    if(transportError || state.collectionError || state.lastRefreshMs===null || !fresh(state.lastRefreshMs) || !agent || !fresh(agent.observedMs))return null;
    if(agent.availability==="notFound")return "notInstalled";
    if(agent.availability==="unsupported")return "unsupported";
    if(agent.authState==="notAuthenticated")return "signInRequired";
    const quota=agent.quota;
    if(!quota || quota.diagnosticCode || !fresh(quota.observedMs))return null;
    if(quota.windows.some(window=>window.windowMinutes===300 && fresh(window.observedMs) && window.resetsAtMs!==null && window.resetsAtMs>now))return "activeWindow";
    if(quota.windows.length && quota.windows.every(window=>window.windowMinutes===10080 && fresh(window.observedMs)))return "weeklyOnly";
    return null;
}
export function renderWakeAction(id:AgentId,state:DashboardState,view?:WakeViewState,transportError=false):HTMLElement {
    const section=node("div","","wake-action");const busy=view?.busy[id];const reason=snapshotWakeReason(id,state,transportError);
    const button=node("button",busy?"Working…":"Wake up","wake-button");button.id=`wake-${id}`;button.type="button";
    button.setAttribute("aria-label",`Wake up ${id==="codex"?"OpenAI Codex":"Claude Code"}`);
    button.disabled=!view?.available || !!busy || reason!==null;
    const detail=node("p",!view?.available?"Wake up is unavailable in this connection.":busy?"Checking or sending a manual wake request…":view.messages[id] ?? (reason?wakeReason(reason):"Confirm a minimal model request."),"wake-detail");
    detail.id=`wake-detail-${id}`;detail.setAttribute("role","status");detail.setAttribute("aria-live","polite");button.setAttribute("aria-describedby",detail.id);section.append(button,detail);return section;
}
export function wakeOutcomeText(outcome:WakeOutcome):string {
    const status={completed:"Request completed.",failed:"Request failed.",timedOut:"Timed out · completion unknown. No automatic retry was sent.",cancelled:"Wake request cancelled.",unavailable:"Wake up unavailable."}[outcome.status];
    const reason=outcome.reason && outcome.status!=="timedOut"?` ${wakeReason(outcome.reason)}`:"";
    const effect={newWindowObserved:"New five-hour window observed after refresh. This does not establish what caused it.",unchanged:"Reported quota unchanged. No new window was observed.",unverified:"Quota effect unverified. A completed request does not prove a window started."}[outcome.quotaEffect];
    return `${status}${reason} ${effect}`;
}
export function renderWakeDialog(offer:WakeOffer,preview:boolean,onConfirm:()=>void,onCancel:()=>void):HTMLElement {
    const overlay=node("div","","wake-overlay");const dialog=node("section","","wake-dialog");dialog.setAttribute("role","dialog");dialog.setAttribute("aria-modal","true");dialog.setAttribute("aria-labelledby","wake-title");dialog.setAttribute("aria-describedby","wake-disclosure");
    const title=node("h2",`Wake up ${offer.agentId==="codex"?"OpenAI Codex":"Claude Code"}?`);title.id="wake-title";
    const description=node("div");description.id="wake-disclosure";
    description.append(node("p",`Model: ${offer.modelLabel}`),node("p","One request uses your subscription usage. It cannot reset an active allowance or guarantee a new five-hour window."),node("p","No saved chat: an isolated session is used without a local conversation transcript. Provider service logs and usage records may remain."));
    if(offer.quotaUnverified)description.append(node("p","Quota is unavailable or incomplete. We cannot verify whether this request starts a usage window.","wake-warning"));
    if(preview)description.append(node("p","Synthetic preview only. No real provider request will be sent.","wake-warning"));
    const prompt=node("pre",WAKE_PROMPT,"wake-prompt");const actions=node("div","","wake-dialog-actions");
    const cancel=node("button","Cancel","wake-button");cancel.type="button";cancel.id="wake-cancel";cancel.addEventListener("click",onCancel);
    const confirm=node("button",preview?"Simulate request":"Send one request","refresh-button");confirm.type="button";confirm.id="wake-confirm";confirm.addEventListener("click",onConfirm);actions.append(cancel,confirm);
    dialog.append(title,description,prompt,actions);overlay.append(dialog);
    overlay.addEventListener("keydown",event=>{
        if(event.key==="Escape"){event.preventDefault();onCancel();}
        if(event.key==="Tab"){
            event.preventDefault();(document.activeElement===cancel?confirm:cancel).focus();
        }
    });
    return overlay;
}
