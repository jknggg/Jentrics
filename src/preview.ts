import {metricPreview} from "./metrics-preview.ts";
import {previewPricing} from "./pricing-preview.ts";
import type {ModelPrice} from "./pricing-types.ts";
import bundledRegistry from "../src-tauri/src/pricing/registry.json" with {type:"json"};
import type {AgentId,Bridge,DashboardState,WakeOffer,WakeOutcome} from "./types.ts";
import {snapshotWakeReason,WAKE_PROMPT} from "./wake-ui.ts";
import {previewHistory} from "./history-preview.ts";
import {previewClaudeBridge} from "./claude-preview.ts";
import {previewAlerts} from "./alerts-preview.ts";
import {previewMonitor} from "./monitor-preview.ts";
import {withPreviewConfiguration} from "./configuration-preview.ts";
import {previewAdapterCatalog} from "./adapter-catalog-preview.ts";

function fixture():DashboardState {
    const observedMs=Date.now();
    const capabilities={discovery:true,authReadiness:true,quotas:false,usage:false,sessionStart:false};
    return {schemaVersion:1,agents:[
        {agentId:"codex",version:"0.162.0-alpha.17.2",availability:"detected",authState:"ready",capabilities:{...capabilities,quotas:true},observedMs,diagnosticCode:null,
            quota:{source:"codexAppServer",observedMs,diagnosticCode:null,windows:[
                {bucketId:"codex:primary",windowMinutes:300,usedPercent:25,resetsAtMs:observedMs+7200000,observedMs},
                {bucketId:"codex:secondary",windowMinutes:10080,usedPercent:42,resetsAtMs:observedMs+259200000,observedMs}
            ]}},
        {agentId:"claude",version:"Synthetic preview",availability:"detected",authState:"ready",capabilities,observedMs,diagnosticCode:null,quota:null}
    ],refreshing:false,lastRefreshMs:observedMs,storageError:null,collectionError:null};
}
export function previewBridge():Bridge {
    let state=fixture();let serial=0;const confirmations=new Map<AgentId,string>();const history=previewHistory();
    const bridge:Bridge={...history,...previewClaudeBridge(),...previewAlerts(),...previewMonitor(),...metricPreview(),...previewPricing(bundledRegistry.models as ModelPrice[]),preview:true,getAdapterCatalog:async()=>previewAdapterCatalog(),getState:async()=>state,subscribe:async()=>()=>{},refresh:async()=>{await new Promise(resolve=>setTimeout(resolve,400));state=fixture();history.record(state);return state;},
        prepareWake:async agentId=>{
            const reason=snapshotWakeReason(agentId,state);const confirmationId=reason?null:`synthetic-${++serial}`;
            if(confirmationId)confirmations.set(agentId,confirmationId);
            return {agentId,enabled:reason===null,reason,modelLabel:"Provider default (synthetic simulation)",prompt:WAKE_PROMPT,quotaUnverified:true,confirmationId} satisfies WakeOffer;
        },
        confirmWake:async(agentId,confirmationId,confirmed)=>{
            const valid=confirmations.get(agentId)===confirmationId;confirmations.delete(agentId);
            return {agentId,status:!confirmed?"cancelled":valid?"completed":"unavailable",reason:!confirmed?"cancelled":valid?null:"expiredConfirmation",quotaEffect:"unverified",dashboard:state} satisfies WakeOutcome;
        }
    };
    return withPreviewConfiguration(bridge,bundledRegistry.models as ModelPrice[]);
}
