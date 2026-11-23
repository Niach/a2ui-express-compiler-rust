// Loads both builds of the package from dist/ the way a Node user would:
// the `--target web` glue, initialised synchronously from the .wasm bytes.
import { readFileSync } from "node:fs";

export async function loadFlavours() {
  const out = {};
  for (const flavour of ["full", "renderer"]) {
    const url = new URL(`../dist/${flavour}/a2ui_express.js`, import.meta.url);
    const mod = await import(url);
    mod.initSync({ module: readFileSync(new URL(`../dist/${flavour}/a2ui_express_bg.wasm`, import.meta.url)) });
    out[flavour] = mod;
  }
  return out;
}
