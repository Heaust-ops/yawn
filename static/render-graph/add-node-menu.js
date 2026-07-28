import { semanticCatalog } from "./catalog.js";

const GROUPS = Object.freeze([
  ["source", "Source"],
  ["expression", "Expression"],
  ["compute", "Compute"],
  ["cpu_preparation", "CPU preparation"],
  ["render", "Render / post"],
  ["frame", "Frame"],
]);

const title = (typeId) => typeId.replaceAll("_", " ").replace(/\b\w/g, (letter) => letter.toUpperCase());

/** Application-owned, immutable add-node catalog model. */
export const addNodeItems = Object.freeze(
  GROUPS.flatMap(([execution, group]) =>
    Object.entries(semanticCatalog)
      .filter(([, definition]) => definition.execution === execution)
      .map(([typeId]) => Object.freeze({ typeId, title: title(typeId), group })),
  ),
);

export function searchAddNodeItems(query, items = addNodeItems) {
  const terms = query.trim().toLocaleLowerCase().split(/\s+/).filter(Boolean);
  return items.filter((item) => terms.every((term) =>
    `${item.title} ${item.typeId} ${item.group}`.toLocaleLowerCase().includes(term),
  ));
}

export function moveAddNodeSelection(index, delta, length) {
  return length ? ((Math.max(0, index) + delta) % length + length) % length : -1;
}

/** Creates one transient DOM menu owned by the application rather than fxnode. */
export function createAddNodeMenu(ownerDocument = document) {
  const ownerWindow = ownerDocument.defaultView;
  const root = ownerDocument.createElement("div");
  root.className = "fxnode-add-menu";
  root.hidden = true;
  root.setAttribute("role", "dialog");
  root.setAttribute("aria-label", "Add render graph node");
  const input = ownerDocument.createElement("input");
  input.type = "search";
  input.placeholder = "Search nodes…";
  input.setAttribute("aria-label", "Search nodes");
  input.setAttribute("aria-controls", "fxnode-add-options");
  input.setAttribute("aria-autocomplete", "list");
  const list = ownerDocument.createElement("div");
  list.id = "fxnode-add-options";
  list.className = "fxnode-add-menu__list";
  list.setAttribute("role", "listbox");
  root.append(input, list);
  ownerDocument.body.append(root);
  let resolve, filtered = addNodeItems, selected = 0, serial = 0, previousFocus;

  const close = (value = null) => {
    if (root.hidden) return;
    root.hidden = true;
    const done = resolve;
    resolve = undefined;
    previousFocus?.focus?.();
    previousFocus = undefined;
    done?.(value);
  };
  const render = () => {
    filtered = searchAddNodeItems(input.value);
    selected = filtered.length ? Math.min(Math.max(selected, 0), filtered.length - 1) : -1;
    list.replaceChildren();
    let group;
    for (const [index, item] of filtered.entries()) {
      if (item.group !== group) {
        group = item.group;
        const heading = ownerDocument.createElement("div");
        heading.className = "fxnode-add-menu__group";
        heading.textContent = group;
        heading.setAttribute("role", "presentation");
        list.append(heading);
      }
      const option = ownerDocument.createElement("button");
      option.type = "button";
      option.id = `fxnode-add-option-${serial}-${index}`;
      option.className = "fxnode-add-menu__option";
      option.dataset.typeId = item.typeId;
      option.textContent = item.title;
      option.setAttribute("role", "option");
      option.setAttribute("aria-selected", String(index === selected));
      option.tabIndex = -1;
      option.addEventListener("pointermove", () => { selected = index; render(); });
      option.addEventListener("click", () => close(item.typeId));
      list.append(option);
    }
    const active = selected >= 0 ? list.querySelector(`[data-type-id="${filtered[selected].typeId}"]`) : null;
    input.setAttribute("aria-activedescendant", active?.id ?? "");
    active?.scrollIntoView({ block: "nearest" });
  };
  const reposition = () => {
    if (root.hidden) return;
    const margin = 8, box = root.getBoundingClientRect();
    root.style.left = `${Math.max(margin, Math.min(Number(root.dataset.x), ownerWindow.innerWidth - box.width - margin))}px`;
    root.style.top = `${Math.max(margin, Math.min(Number(root.dataset.y), ownerWindow.innerHeight - box.height - margin))}px`;
  };
  input.addEventListener("input", () => { selected = 0; render(); });
  input.addEventListener("keydown", (event) => {
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault(); selected = moveAddNodeSelection(selected, event.key === "ArrowDown" ? 1 : -1, filtered.length); render();
    } else if (event.key === "Enter" && selected >= 0) {
      event.preventDefault(); close(filtered[selected].typeId);
    } else if (event.key === "Escape") { event.preventDefault(); close(); }
  });
  const outside = (event) => { if (!root.hidden && !root.contains(event.target)) close(); };
  ownerDocument.addEventListener("pointerdown", outside, true);
  ownerWindow.addEventListener("resize", close);
  ownerWindow.addEventListener("blur", close);
  return {
    open({ x, y }) {
      close();
      serial++;
      previousFocus = ownerDocument.activeElement;
      root.dataset.x = String(x); root.dataset.y = String(y);
      input.value = ""; selected = 0; root.hidden = false; render(); reposition(); input.focus();
      return new Promise((done) => { resolve = done; });
    },
    close,
    destroy() {
      close();
      ownerDocument.removeEventListener("pointerdown", outside, true);
      ownerWindow.removeEventListener("resize", close);
      ownerWindow.removeEventListener("blur", close);
      root.remove();
    },
  };
}
