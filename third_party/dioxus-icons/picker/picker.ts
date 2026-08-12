interface IconEntry {
  name: string;
  svg: string;
  tags?: string[];
  categories?: string[];
}

interface IconManifest {
  icons: IconEntry[];
}

type Rank = 0 | 1 | 2 | 9;

(function init(): void {
  const manifestNode = document.getElementById("__icon_manifest__");
  if (!manifestNode) return;

  let manifest: IconManifest;
  try {
    manifest = JSON.parse(manifestNode.textContent || '{"icons":[]}') as IconManifest;
  } catch {
    return;
  }
  const icons: IconEntry[] = Array.isArray(manifest.icons) ? manifest.icons : [];

  const roots = Array.from(
    document.querySelectorAll<HTMLElement>("[data-dxi-picker]"),
  );
  if (!roots.length) return;

  const fallback = document.getElementById("dioxus-icons-static-picker");
  if (fallback) (fallback as HTMLElement).style.display = "none";

  roots.forEach((root) => {
    root.hidden = false;
    bindPicker(root, icons);
  });

  document.addEventListener("keydown", (event: KeyboardEvent) => {
    if (event.key !== "/" || isTyping(event.target)) return;
    for (const root of roots) {
      const input = root.querySelector<HTMLInputElement>("[data-dxi-input]");
      if (input && input.offsetParent !== null) {
        event.preventDefault();
        input.focus();
        input.select();
        return;
      }
    }
  });
})();

function isTyping(target: EventTarget | null): boolean {
  if (!target || !(target instanceof HTMLElement)) return false;
  const tag = target.tagName;
  return tag === "INPUT" || tag === "TEXTAREA" || target.isContentEditable;
}

function bindPicker(root: HTMLElement, icons: IconEntry[]): void {
  const input = root.querySelector<HTMLInputElement>("[data-dxi-input]");
  const clearBtn = root.querySelector<HTMLButtonElement>("[data-dxi-clear]");
  const count = root.querySelector<HTMLElement>("[data-dxi-count] strong");
  const grid = root.querySelector<HTMLElement>("[data-dxi-grid]");
  const scroller = root.querySelector<HTMLElement>("[data-dxi-scroll]");
  if (!input || !grid || !scroller) return;

  let activeIndex = 0;
  let filtered: IconEntry[] = icons.slice();

  function lower(value: unknown): string {
    return String(value ?? "").toLowerCase();
  }

  function rank(icon: IconEntry, query: string): Rank {
    if (!query) return 0;
    if (lower(icon.name).indexOf(query) !== -1) return 0;
    for (const tag of icon.tags ?? []) {
      if (lower(tag).indexOf(query) !== -1) return 1;
    }
    for (const category of icon.categories ?? []) {
      if (lower(category).indexOf(query) !== -1) return 2;
    }
    return 9;
  }

  function hrefFor(icon: IconEntry): string {
    return "lucide/fn." + icon.name + ".html";
  }

  function buildCell(icon: IconEntry, index: number): HTMLAnchorElement {
    const link = document.createElement("a");
    link.className = "dxi-picker-cell";
    link.href = hrefFor(icon);
    link.dataset.index = String(index);
    link.dataset.active = index === activeIndex ? "true" : "false";
    link.tabIndex = index === activeIndex ? 0 : -1;
    const tagSummary = (icon.tags ?? []).slice(0, 4).join(", ");
    link.title = tagSummary ? icon.name + " — " + tagSummary : icon.name;

    const preview = document.createElement("span");
    preview.className = "dxi-picker-cell-preview";
    preview.innerHTML = icon.svg || "";

    const name = document.createElement("span");
    name.className = "dxi-picker-cell-name";
    name.textContent = icon.name;

    link.appendChild(preview);
    link.appendChild(name);
    return link;
  }

  function renderEmpty(): void {
    const empty = document.createElement("div");
    empty.className = "dxi-picker-empty";
    empty.innerHTML =
      '<svg class="dxi-picker-empty-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><circle cx="11" cy="11" r="8"></circle><path d="m21 21-4.3-4.3"></path><path d="M8.5 11h5"></path></svg>';
    const title = document.createElement("div");
    title.className = "dxi-picker-empty-title";
    title.textContent = "No icons match";
    const hint = document.createElement("div");
    hint.className = "dxi-picker-empty-hint";
    hint.textContent = "Try a shorter or different search term.";
    empty.appendChild(title);
    empty.appendChild(hint);
    grid!.appendChild(empty);
  }

  function render(): void {
    grid!.textContent = "";
    if (count) count.textContent = String(filtered.length);
    if (!filtered.length) {
      renderEmpty();
      return;
    }
    const frag = document.createDocumentFragment();
    for (let i = 0; i !== filtered.length; i += 1) {
      frag.appendChild(buildCell(filtered[i], i));
    }
    grid!.appendChild(frag);
  }

  function filter(): void {
    const query = lower(input!.value).trim();
    filtered = icons
      .map((icon) => ({ icon, rank: rank(icon, query) }))
      .filter((item) => item.rank !== 9)
      .sort((a, b) =>
        a.rank !== b.rank ? a.rank - b.rank : a.icon.name.localeCompare(b.icon.name),
      )
      .map((item) => item.icon);
    activeIndex = 0;
    if (clearBtn) clearBtn.hidden = !query.length;
    render();
    scroller!.scrollTop = 0;
  }

  function columnsCount(): number {
    const computed = window.getComputedStyle(grid!);
    const template = computed.gridTemplateColumns;
    if (!template || template === "none") return 1;
    return Math.max(1, template.split(" ").filter((token) => token.length > 0).length);
  }

  function setActive(nextIndex: number): void {
    if (!filtered.length) return;
    const cells = grid!.children;
    const prev = cells[activeIndex] as HTMLElement | undefined;
    if (prev) {
      prev.dataset.active = "false";
      prev.tabIndex = -1;
    }
    activeIndex = Math.max(0, Math.min(filtered.length - 1, nextIndex));
    const cell = cells[activeIndex] as HTMLElement | undefined;
    if (!cell) return;
    cell.dataset.active = "true";
    cell.tabIndex = 0;
    cell.scrollIntoView({ block: "nearest", inline: "nearest" });
    cell.focus({ preventScroll: true });
  }

  function handleNavigation(event: KeyboardEvent): void {
    if (event.altKey || event.ctrlKey || event.metaKey) return;
    const target = event.target;
    const inPicker =
      target === input ||
      target === scroller ||
      (target instanceof Node && grid!.contains(target));
    if (!inPicker) return;

    if (event.key === "ArrowDown") {
      event.preventDefault();
      setActive(activeIndex + columnsCount());
    } else if (event.key === "ArrowUp") {
      event.preventDefault();
      setActive(activeIndex - columnsCount());
    } else if (event.key === "ArrowRight") {
      event.preventDefault();
      setActive(activeIndex + 1);
    } else if (event.key === "ArrowLeft") {
      event.preventDefault();
      setActive(activeIndex - 1);
    } else if (event.key === "Enter") {
      const cell =
        target instanceof HTMLElement
          ? target.closest<HTMLAnchorElement>(".dxi-picker-cell")
          : null;
      const href = cell ? cell.href : (grid!.children[activeIndex] as HTMLAnchorElement | undefined)?.href;
      if (href) {
        event.preventDefault();
        window.location.href = href;
      }
    } else if (event.key === "Escape") {
      event.preventDefault();
      if (input!.value) {
        input!.value = "";
        filter();
      } else if (target === input) {
        input!.blur();
      } else {
        input!.focus();
        input!.select();
      }
    }
  }

  input.addEventListener("input", filter);
  root.addEventListener("keydown", handleNavigation);

  if (clearBtn) {
    clearBtn.addEventListener("click", () => {
      input.value = "";
      filter();
      input.focus();
    });
  }

  filter();
}
