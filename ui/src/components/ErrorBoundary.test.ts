import { describe, expect, test } from "bun:test";
import type { VNode } from "preact";
import { ErrorBoundary } from "./ErrorBoundary";

// No DOM (bun test) / no @testing-library here, so we assert on the vnode
// tree instead of a rendered document: getDerivedStateFromError drives the
// boundary into its fallback branch, then we walk the fallback's div vnodes.

function classNames(vnode: VNode): string[] {
  const out: string[] = [];
  const walk = (v: unknown) => {
    if (!v || typeof v !== "object") return;
    const node = v as VNode;
    if (node.type === "div") out.push(String((node.props as Record<string, unknown>).class ?? ""));
    const children = node.props?.children;
    (Array.isArray(children) ? children : [children]).forEach(walk);
  };
  walk(vnode);
  return out;
}

function collectTexts(vnode: VNode): string[] {
  const out: string[] = [];
  const walk = (v: unknown) => {
    if (typeof v === "string") out.push(v);
    else if (Array.isArray(v)) v.forEach(walk);
    else if (v && typeof v === "object") walk((v as VNode).props?.children);
  };
  walk(vnode);
  return out;
}

describe("ErrorBoundary", () => {
  test("renders children when no error", () => {
    const b = new ErrorBoundary({ children: "ok-child" }, {});
    expect(b.render()).toBe("ok-child");
  });

  test("getDerivedStateFromError returns the error as state", () => {
    const err = new Error("boom-boom");
    expect(ErrorBoundary.getDerivedStateFromError(err)).toEqual({ error: err });
  });

  test("render error state shows 500-ish fallback", () => {
    const err = new Error("kaboom-message");
    const b = new ErrorBoundary({ children: "never" }, {});
    b.state = ErrorBoundary.getDerivedStateFromError(err);
    const v = b.render() as VNode;

    const classes = classNames(v).join(" ");
    expect(classes).toContain("min-h-[60vh]");
    const all = collectTexts(v);
    expect(all).toContain("kaboom-message");
    expect(all).toContain("try again");
    expect(all).toContain("500");
  });
});
