import {createConfigurationController} from "./configuration-controller.ts";
import {createDisplayController} from "./display-controller.ts";
import {createCountdownUpdater} from "./countdown.ts";
import {createAdapterCatalogController} from "./adapter-catalog-ui.ts";
import {createSettingsNavigation} from "./settings-navigation.ts";
import type {DisplaySettings} from "./configuration-types.ts";
import {createMetricsController} from "./metrics-controller.ts";
import {createPricingController} from "./pricing-controller.ts";
import type { Bridge, DashboardState } from "./types.ts";
import {renderDashboard} from "./render.ts";
import {createWakeController} from "./wake-controller.ts";
import {createHistoryController} from "./history-controller.ts";
import {createClaudeController} from "./claude-controller.ts";
import {createAlertsController} from "./alerts-controller.ts";
import {createMonitorController} from "./monitor-controller.ts";
import {renderMiniDashboard} from "./mini-dashboard.ts";
import {createDashboardPresentation} from "./dashboard-presentation.ts";
import type {DashboardWindowControls} from "./dashboard-presentation.ts";

export async function mountDashboard(root: HTMLElement, bridge: Bridge, windowControls?: DashboardWindowControls): Promise<{ dispose(): void }> {
    let state:DashboardState={schemaVersion:1,agents:[],refreshing:false,lastRefreshMs:null,storageError:null,collectionError:null};
    let primary:DisplaySettings|null=null;let disposed=false;let busy=false;let eventRevision=0;let transportError=false;let unlisten:(()=>void)|undefined;
    const content=document.createElement("div");root.replaceChildren(content);
    const display=createDisplayController(bridge,()=>state,settings=>{primary=settings;paint();});
    const history=createHistoryController(root,content,bridge,()=>{void configuration.reloadMaintenance();});
    const claude=createClaudeController(root,content,bridge);
    const alerts=createAlertsController(bridge);
    let presentation: ReturnType<typeof createDashboardPresentation> | undefined;
    const monitor=createMonitorController(bridge,()=>{if(presentation?.active())paint();});
    const metrics=createMetricsController(root,content,bridge,()=>{void configuration.reloadMaintenance();});
    const pricing=createPricingController(bridge);
    const catalog=createAdapterCatalogController(bridge);
    const configuration=createConfigurationController(root,content,bridge,async()=>{await Promise.all([history.reload(),monitor.reload(),metrics.reload(),alerts.refreshStatus(),pricing.reload(),display.reload(),catalog.reload()]);});
    const navigation=createSettingsNavigation(root,bridge,()=>presentation!.expand());
    const wake=createWakeController(root,content,bridge,()=>state,next=>{eventRevision++;state=next;},paint);
    presentation=createDashboardPresentation(windowControls,paint);
    function paint(){
        if(disposed)return;
        const focusId=content.contains(document.activeElement)?document.activeElement?.id:undefined;
        const current={...state,refreshing:state.refreshing || busy};
        if(presentation!.active()){
            content.replaceChildren(renderMiniDashboard(current,{...presentation!.view(),preview:bridge.preview,transportError,monitor:monitor.snapshot()}));
        }else{
        content.replaceChildren(renderDashboard(current,{preview:bridge.preview,transportError,wake:wake.view,display:primary}));
        content.querySelector(".dashboard")!.insertBefore(history.element,content.querySelector(".privacy-note"));
        content.querySelector(".dashboard")!.insertBefore(claude.element,history.element);
        content.querySelector(".dashboard")!.insertBefore(alerts.element,content.querySelector(".privacy-note"));
        content.querySelector(".dashboard")!.insertBefore(monitor.element,alerts.element);
        content.querySelector(".dashboard")!.insertBefore(metrics.element,monitor.element);
        content.querySelector(".dashboard")!.insertBefore(pricing.element,monitor.element);
        const quota=content.querySelector(".quota-sections")!;quota.after(display.element);
        content.querySelector(".dashboard")!.insertBefore(configuration.element,content.querySelector(".privacy-note"));
        content.querySelector(".dashboard")!.insertBefore(catalog.element,configuration.element);
        display.refreshSnapshot();
        const intro=content.querySelector(".intro")!;
        const refresh=content.querySelector("#refresh")!;
        const actions=document.createElement("div");actions.className="intro-actions";
        const shrink=document.createElement("button");shrink.id="mini-open";shrink.type="button";shrink.className="mini-open-button";shrink.textContent="Mini view";shrink.disabled=presentation!.view().busy;
        actions.append(refresh,shrink);intro.append(actions);
        if(presentation!.view().error){const notice=document.createElement("p");notice.className="mini-window-error";notice.setAttribute("role","alert");notice.textContent=presentation!.view().error;intro.after(notice);}
        }
        presentation!.bind(content);
        const button=root.querySelector<HTMLButtonElement>("#refresh")!;
        button.addEventListener("click",async()=>{
            if(busy || state.refreshing || disposed)return;
            busy=true;transportError=false;paint();const revision=eventRevision;
            try{const next=await bridge.refresh();if(eventRevision===revision){state=next;void history.reload();}}
            catch{transportError=true;}
            finally{busy=false;paint();root.querySelector<HTMLButtonElement>("#refresh")?.focus();}
        });
        wake.bind();
        if(focusId){const focused=content.querySelector<HTMLButtonElement>(`#${focusId}`);if(focused && !focused.disabled)focused.focus();}
    }
    paint();
    try{unlisten=await bridge.subscribe(next=>{eventRevision++;state=next;transportError=false;paint();if(!next.refreshing){void history.reload();void monitor.reload();void metrics.reload();void alerts.refreshStatus();void catalog.reload();void configuration.refreshStatus();}});}
    catch{transportError=true;}
    const revision=eventRevision;
    try{const initial=await bridge.getState();if(eventRevision===revision)state=initial;}
    catch{transportError=true;}
    paint();
    await Promise.all([history.start(),claude.start(),alerts.start(),monitor.start(),metrics.start(),pricing.start(),display.start(),catalog.start(),configuration.start(),navigation.start()]);
    const countdown=createCountdownUpdater(root);
    const agingTimer=setInterval(()=>{if(!document.hidden){paint();void monitor.reload();}},60_000);
    return {dispose(){disposed=true;presentation!.dispose();wake.dispose();history.dispose();claude.dispose();alerts.dispose();monitor.dispose();metrics.dispose();pricing.dispose();display.dispose();catalog.dispose();configuration.dispose();navigation.dispose();countdown.dispose();clearInterval(agingTimer);unlisten?.();}};
}
