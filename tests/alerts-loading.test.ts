import {test} from "node:test";
import assert from "node:assert/strict";
import {parseHTML} from "linkedom";
import {createAlertsController} from "../src/alerts-controller.ts";
import type {AlertSettingsView} from "../src/alerts-types.ts";

function view():AlertSettingsView {
    return {settings:{enabled:false,threshold80:true,threshold95:true,nearReset:true,expectedReset:true,restored:true,errors:true,quietHours:null},permission:"granted",error:null};
}
function deferred<T>() {
    let resolve!:(value:T)=>void;let reject!:(error:Error)=>void;
    const promise=new Promise<T>((yes,no)=>{resolve=yes;reject=no;});
    return {promise,resolve,reject};
}
const flush=()=>new Promise(resolve=>setTimeout(resolve,0));

for (const explicitReload of [false,true]) {
    test(`passive alert refresh cannot strand a pending ${explicitReload?"explicit reload":"startup load"}`,async()=>{
        for (const fails of [false,true]) {
            const {document}=parseHTML("<html><body></body></html>");
            Object.defineProperty(globalThis,"document",{value:document,configurable:true});
            const pending=deferred<AlertSettingsView>();
            let reads=0;let writes=0;
            const pendingRead=explicitReload?2:1;
            const controller=createAlertsController({
                getAlertSettings:async()=>{
                    reads++;
                    if(reads===pendingRead)return pending.promise;
                    if(reads>pendingRead && fails)throw new Error("PRIVATE_STATUS_FAILURE");
                    return view();
                },
                setAlertSettings:async()=>{writes++;return view();}
            });
            document.body.append(controller.element);
            try {
                let startup:Promise<void>|undefined;
                if(explicitReload) {
                    await controller.start();
                    controller.element.querySelector<HTMLButtonElement>("#alerts-reload")!.click();
                } else {startup=controller.start();}
                assert.equal(reads,pendingRead);
                await controller.refreshStatus();
                if(fails)pending.reject(new Error("PRIVATE_LOAD_FAILURE"));else pending.resolve(view());
                await startup;await flush();
                const reload=controller.element.querySelector<HTMLButtonElement>("#alerts-reload")!;
                assert.equal(reload.disabled,false,"An overlapping passive read left Reload disabled");
                assert.equal(reads,pendingRead,"Passive refresh should defer to the active load");
                assert.equal(writes,0);
                if(fails) {
                    assert.match(controller.element.textContent??"",/settings could not be loaded/i);
                    assert.doesNotMatch(controller.element.textContent??"",/PRIVATE_/);
                } else {
                    assert.equal(controller.element.querySelector<HTMLButtonElement>("#alerts-save")!.disabled,false);
                }
                // After either outcome a deliberate reload remains usable.
                reload.click();await flush();
                assert.equal(controller.element.querySelector<HTMLButtonElement>("#alerts-reload")!.disabled,false);
            } finally {controller.dispose();}
        }
    });
}
