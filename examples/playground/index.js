import { PLAYGROUND_RECIPES, playgroundRecipe } from "./recipes.js";

const select = document.querySelector("#recipe-select");
const title = document.querySelector("#recipe-title");
const packageName = document.querySelector("#recipe-package");
const editor = document.querySelector("#editor");
const preview = document.querySelector("#preview");
const status = document.querySelector("#status");
const docs = document.querySelector("#docs-link");
let recipeId;

for (const [id, recipe] of Object.entries(PLAYGROUND_RECIPES)) {
  const option = document.createElement("option");
  option.value = id;
  option.textContent = recipe.title;
  select.append(option);
}

function choose(id, updateUrl = true) {
  recipeId = PLAYGROUND_RECIPES[id] ? id : "first-scene";
  const recipe = playgroundRecipe(recipeId);
  select.value = recipeId;
  title.textContent = recipe.title;
  packageName.textContent = recipe.package;
  editor.value = recipe.source;
  docs.href = recipe.docs;
  status.textContent = recipe.description;
  if (updateUrl) {
    const url = new URL(location.href);
    url.searchParams.set("recipe", recipeId);
    history.replaceState(null, "", url);
  }
  run();
}

function run() {
  status.textContent = "Running…";
  status.dataset.error = "false";
  preview.src = `./runner.html?recipe=${encodeURIComponent(recipeId)}&run=${Date.now()}`;
}

addEventListener("message", (event) => {
  if (event.origin !== location.origin || event.source !== preview.contentWindow) return;
  if (event.data?.type === "playground-runner-ready") {
    preview.contentWindow.postMessage(
      { type: "playground-run", source: editor.value },
      location.origin,
    );
  } else if (event.data?.type === "playground-status" || event.data?.type === "playground-error") {
    status.textContent = event.data.message;
    status.dataset.error = String(event.data.type === "playground-error");
  }
});

document.querySelector("#run").addEventListener("click", run);
document.querySelector("#reset").addEventListener("click", () => {
  editor.value = playgroundRecipe(recipeId).source;
  run();
});
document.querySelector("#copy").addEventListener("click", async (event) => {
  await navigator.clipboard.writeText(location.href);
  const original = event.currentTarget.textContent;
  event.currentTarget.textContent = "Copied";
  setTimeout(() => { event.currentTarget.textContent = original; }, 1200);
});
select.addEventListener("change", () => choose(select.value));
editor.addEventListener("keydown", (event) => {
  if (event.key === "Enter" && (event.ctrlKey || event.metaKey)) {
    event.preventDefault();
    run();
  }
  if (event.key === "Tab") {
    event.preventDefault();
    const start = editor.selectionStart;
    editor.setRangeText("  ", start, editor.selectionEnd, "end");
  }
});

choose(new URLSearchParams(location.search).get("recipe"), false);
