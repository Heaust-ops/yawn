import type { FxNodeView } from "@lib/index.js";

/** Application-owned add-node menu used by the standalone examples. */

interface Point {
  readonly x: number;
  readonly y: number;
}

export interface AddNodeMenu {
  open(viewPosition: Point): void;
  close(restoreCanvasFocus?: boolean): void;
  destroy(): void;
}

function cloneElement<T extends Element>(template: HTMLTemplateElement, selector: string): T {
  const element = template.content.firstElementChild?.cloneNode(true);
  if (!(element instanceof Element)) throw new Error(`Add-node menu template ${selector} is empty`);
  return element as T;
}

export function createAddNodeMenu(
  template: HTMLTemplateElement,
  canvas: HTMLCanvasElement,
  api: FxNodeView,
  onError: (error: unknown) => void,
): AddNodeMenu {
  const entries = [...template.content.querySelectorAll<HTMLButtonElement>("button[data-fxnode-menu-option]")].map(
    (option) => {
      const typeId = option.dataset.typeId;
      const title = option.textContent?.trim();
      const group = option.closest<HTMLElement>("[data-fxnode-menu-group]");
      const groupTitle = group?.querySelector<HTMLElement>("[data-fxnode-menu-heading]")?.textContent?.trim();
      if (!typeId || !title || !group || !groupTitle) throw new Error("Add-node menu option is missing HTML context");
      return {
        typeId,
        searchText: `${title} ${groupTitle} ${typeId} ${option.dataset.keywords ?? ""}`.toLowerCase(),
        option,
      };
    },
  );
  if (!entries.length) throw new Error("Add-node menu HTML template has no node options");
  let host: HTMLDivElement | undefined;
  let removeListeners = () => {};

  const close = (restore = false) => {
    if (!host) return;
    removeListeners();
    host.remove();
    host = undefined;
    if (restore) canvas.focus();
  };

  const open = (viewPosition: Point) => {
    close();
    host = cloneElement<HTMLDivElement>(template, "root");
    const input = host.querySelector<HTMLInputElement>("[data-fxnode-menu-search]");
    const results = host.querySelector<HTMLDivElement>("[data-fxnode-menu-results]");
    const empty = host.querySelector<HTMLElement>("[data-fxnode-menu-empty]");
    const options = [...host.querySelectorAll<HTMLButtonElement>("button[data-fxnode-menu-option]")];
    if (!input || !results || !empty || options.length !== entries.length) {
      host.remove();
      host = undefined;
      throw new Error("Add-node menu HTML template is incomplete");
    }

    let visible = entries.slice();
    let active = 0;
    const choose = (index: number) => {
      const item = visible[index];
      if (!item) return;
      close(true);
      void api.addNode({ typeId: item.typeId, viewPosition }).catch(onError);
    };
    const render = () => {
      entries.forEach((item, sourceIndex) => {
        const button = options[sourceIndex]!;
        const index = visible.indexOf(item);
        button.hidden = index < 0;
        if (index >= 0) {
          button.classList.toggle("active", index === active);
          button.id = `fxnode-node-option-${index}`;
          button.setAttribute("aria-selected", String(index === active));
          button.onpointerenter = () => {
            if (active !== index) {
              active = index;
              render();
            }
          };
          button.onclick = () => choose(index);
        }
      });
      for (const group of results.querySelectorAll<HTMLElement>("[data-fxnode-menu-group]"))
        group.hidden = !group.querySelector("button[data-fxnode-menu-option]:not([hidden])");
      empty.hidden = visible.length > 0;
      input.setAttribute("aria-expanded", "true");
      input.setAttribute("aria-activedescendant", visible.length ? `fxnode-node-option-${active}` : "");
    };
    input.oninput = () => {
      const query = input.value.trim().toLowerCase();
      visible = entries.filter((item) => !query || item.searchText.includes(query));
      active = 0;
      render();
    };
    input.onkeydown = (event) => {
      if (event.key === "Escape") {
        event.preventDefault();
        close(true);
        return;
      }
      if (!visible.length) return;
      if (event.key === "ArrowDown" || event.key === "ArrowUp" || event.key === "Home" || event.key === "End") {
        event.preventDefault();
        active =
          event.key === "Home"
            ? 0
            : event.key === "End"
              ? visible.length - 1
              : (active + (event.key === "ArrowDown" ? 1 : -1) + visible.length) % visible.length;
        render();
        host?.querySelector(`#fxnode-node-option-${active}`)?.scrollIntoView({ block: "nearest" });
      } else if (event.key === "Enter") {
        event.preventDefault();
        choose(active);
      }
    };

    render();
    document.body.append(host);
    const rect = canvas.getBoundingClientRect();
    const menu = host.getBoundingClientRect();
    const margin = 8;
    host.style.left = `${Math.max(margin, Math.min(rect.left + viewPosition.x, window.innerWidth - menu.width - margin))}px`;
    host.style.top = `${Math.max(margin, Math.min(rect.top + viewPosition.y, window.innerHeight - menu.height - margin))}px`;
    host.style.visibility = "visible";
    input.focus();
    const outside = (event: PointerEvent) => {
      if (host && !event.composedPath().includes(host)) close(false);
    };
    const blur = () => close(false);
    const scroll = () => close(false);
    document.addEventListener("pointerdown", outside, true);
    window.addEventListener("blur", blur);
    window.addEventListener("scroll", scroll, true);
    removeListeners = () => {
      document.removeEventListener("pointerdown", outside, true);
      window.removeEventListener("blur", blur);
      window.removeEventListener("scroll", scroll, true);
      removeListeners = () => {};
    };
  };

  return { open, close, destroy: () => close(false) };
}
