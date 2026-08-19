import { createPlaygroundRuntime } from "./runtime.js";
import { playgroundRecipe } from "./recipes.js";

const status = document.querySelector("#status");
const canvas = document.querySelector("#scene");
const parentOrigin = location.origin;
const parameters = new URLSearchParams(location.search);
const embedded = parameters.has("embed");
document.body.dataset.embed = String(embedded);
let started = false;

function report(message, error = false) {
  status.textContent = message;
  document.body.dataset.error = String(error);
  parent.postMessage({ type: error ? "playground-error" : "playground-status", message }, parentOrigin);
}

async function execute(source) {
  if (started) return;
  started = true;
  try {
    const yawn = createPlaygroundRuntime(canvas, report);
    const AsyncFunction = Object.getPrototypeOf(async function () {}).constructor;
    await new AsyncFunction("yawn", `"use strict";\n${source}`)(yawn);
    document.documentElement.dataset.yawnReady = "true";
    parent.postMessage({ type: "playground-ready" }, parentOrigin);
  } catch (error) {
    console.error(error);
    report(error?.stack ?? error?.message ?? String(error), true);
  }
}

addEventListener("message", (event) => {
  if (event.origin !== parentOrigin || event.data?.type !== "playground-run") return;
  void execute(event.data.source);
});

const recipeId = parameters.get("recipe");
if (embedded) {
  void execute(playgroundRecipe(recipeId).source);
} else {
  parent.postMessage({ type: "playground-runner-ready" }, parentOrigin);
}
