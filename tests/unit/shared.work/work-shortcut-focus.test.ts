import { isValidElement, type MouseEventHandler, type ReactElement, type ReactNode } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";

const hooks = vi.hoisted(() => ({
  slots: [] as { value: unknown }[], cursor: 0, layouts: [] as (() => void)[],
}));
vi.mock("react", async (original) => ({
  ...await original<typeof import("react")>(),
  useId: () => "shortcut-editor",
  useState<T>(initial: T | (() => T)) {
    const index = hooks.cursor++;
    hooks.slots[index] ??= { value: typeof initial === "function" ? (initial as () => T)() : initial };
    return [hooks.slots[index].value as T, (update: T | ((value: T) => T)) => {
      hooks.slots[index].value = typeof update === "function"
        ? (update as (value: T) => T)(hooks.slots[index].value as T) : update;
    }];
  },
  useRef<T>(initial: T) {
    const index = hooks.cursor++;
    hooks.slots[index] ??= { value: { current: initial } };
    return hooks.slots[index].value;
  },
  useMemo<T>(calculate: () => T) { return calculate(); },
  useLayoutEffect(effect: () => void, dependencies: unknown[]) {
    const index = hooks.cursor++, previous = hooks.slots[index]?.value as unknown[] | undefined;
    if (!previous || dependencies.some((value, key) => !Object.is(value, previous[key]))) hooks.layouts.push(effect);
    hooks.slots[index] = { value: dependencies };
  },
}));
vi.mock("@erpx/ui/components/foundation", () => ({ Button: "button", Field: "label", TextInput: "input", ActionLink: "a" }));
vi.mock("@erpx/ui/components/dialog", () => ({ Dialog: "dialog" }));
vi.mock("@erpx/feature-work_projection/components/quick-access/use-local-preference", () => ({
  useLocalShortcutPreference: (_key: unknown, preference: unknown) => ({ preference }),
}));

import { QuickAccessShelf } from "@erpx/feature-work_projection/components/quick-access/shelf";

type NodeProps = {
  children?: ReactNode; open?: boolean; disabled?: boolean; "aria-label"?: string;
  onClick?: MouseEventHandler<HTMLButtonElement>;
  ref?: (element: HTMLButtonElement | null) => void;
};
type Control = {
  disabled: boolean; isConnected: boolean;
  ownerDocument: { activeElement: unknown };
  focus: () => void;
  props: NodeProps;
};
function editor(ids: string[]) {
  const outside = {}, document = { activeElement: outside as unknown };
  const controls = new Map<string, Control>();
  const props: Parameters<typeof QuickAccessShelf>[0] = {
    items: ids.map((id) => ({ id, title: id, href: `/work/${id}`, tone: "brand" })),
    initialPreferences: { pinnedIds: ids, version: 1 }, defaultIds: ids,
    endpoint: "/preferences", title: "My shortcuts", appearance: "standard",
  };
  function render() {
    hooks.cursor = 0;
    const wrapper = QuickAccessShelf(props);
    const state = wrapper.type as (input: typeof props) => ReactElement;
    const tree = state(wrapper.props);
    const live = new Set<string>();
    function walk(node: ReactNode) {
      if (Array.isArray(node)) { node.forEach(walk); return; }
      if (!isValidElement<NodeProps>(node)) return;
      if (node.type === "dialog" && !node.props.open) return;
      const label = node.props["aria-label"];
      if (node.type === "button" && label) {
        live.add(label);
        let control = controls.get(label);
        if (!control) {
          control = { disabled: false, isConnected: true, ownerDocument: document, props: node.props, focus: () => { document.activeElement = control; } };
          controls.set(label, control);
        }
        control.props = node.props;
        control.disabled = !!node.props.disabled;
        control.isConnected = true;
        node.props.ref?.(control as unknown as HTMLButtonElement);
      }
      walk(node.props.children);
    }
    walk(tree);
    for (const [label, control] of controls) if (!live.has(label)) control.isConnected = false;
    // Model the browser's observed blur when its active native button is disabled.
    const active = document.activeElement as Control | undefined;
    if (active?.disabled || active?.isConnected === false) document.activeElement = outside;
    const effects = hooks.layouts.splice(0);
    effects.forEach((effect) => effect());
  }
  function get(label: string) {
    const control = controls.get(label);
    if (!control?.isConnected) throw Error(`Missing control: ${label}`);
    return control;
  }
  function activate(label: string, focus = true) {
    const control = get(label);
    if (control.disabled) throw Error(`Disabled control: ${label}`);
    if (focus) control.focus();
    control.props.onClick?.({ currentTarget: control } as unknown as Parameters<MouseEventHandler<HTMLButtonElement>>[0]);
    render();
  }
  render();
  activate("Customize");
  return { activate, get, document, outside };
}
beforeEach(() => { hooks.slots = []; hooks.cursor = 0; hooks.layouts = []; });

describe("Work shortcut reorder focus", () => {
  it("keeps keyboard focus on the moved row's enabled down control at the top boundary", () => {
    const view = editor(["expenses", "services"]);
    view.activate("Move up: services");
    expect(view.get("Move up: services").disabled).toBe(true);
    expect(view.get("Move down: services").disabled).toBe(false);
    expect(view.document.activeElement).toBe(view.get("Move down: services"));
  });

  it("keeps keyboard focus on the moved row's enabled up control at the bottom boundary", () => {
    const view = editor(["expenses", "services"]);
    view.activate("Move down: expenses");
    expect(view.get("Move down: expenses").disabled).toBe(true);
    expect(view.get("Move up: expenses").disabled).toBe(false);
    expect(view.document.activeElement).toBe(view.get("Move up: expenses"));
  });

  it("retains the same enabled up control when the moved row remains inside the list", () => {
    const view = editor(["a", "b", "c", "d"]);
    view.activate("Move up: c");
    expect(view.get("Move up: c").disabled).toBe(false);
    expect(view.document.activeElement).toBe(view.get("Move up: c"));
  });

  it("retains the same enabled down control when the moved row remains inside the list", () => {
    const view = editor(["a", "b", "c", "d"]);
    view.activate("Move down: b");
    expect(view.get("Move down: b").disabled).toBe(false);
    expect(view.document.activeElement).toBe(view.get("Move down: b"));
  });

  it("does not steal focus when an activated reorder control did not own focus", () => {
    const view = editor(["expenses", "services"]);
    view.document.activeElement = view.outside;
    view.activate("Move up: services", false);
    expect(view.get("Move up: services").disabled).toBe(true);
    expect(view.document.activeElement).toBe(view.outside);
  });

  it("consumes the reorder focus request once and leaves later draft edits' focus intact", () => {
    const view = editor(["expenses", "services"]);
    view.activate("Move up: services");
    expect(view.document.activeElement).toBe(view.get("Move down: services"));
    view.document.activeElement = view.outside;
    view.activate("Remove: expenses", false);
    expect(view.document.activeElement).toBe(view.outside);
  });
});
