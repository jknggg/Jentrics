import{test}from"node:test";import assert from"node:assert/strict";import{nativeBridge}from"../src/native-bridge.ts";
test("native dashboard does not expose Wake up under the retained retry/isolation policy",()=>{const bridge=nativeBridge();assert.equal(bridge.prepareWake,undefined);assert.equal(bridge.confirmWake,undefined);});
