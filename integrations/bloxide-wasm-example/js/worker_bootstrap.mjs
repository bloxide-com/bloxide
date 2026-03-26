// ES-module worker: place next to wasm-pack `pkg/` output and fix the import path.
import init, { initBloxideApp } from "../pkg/bloxide_wasm_example.js";

self.onmessage = async (event) => {
  if (event.data?.type !== "bloxide-start" || !event.data.port) {
    return;
  }
  await init();
  initBloxideApp(event.data.port);
};
