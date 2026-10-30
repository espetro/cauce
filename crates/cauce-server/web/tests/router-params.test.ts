// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// `setParams` mirrors UI toggles (home mode, sources panel) into the URL
// so agents and deep links can land on the state directly.

import { afterEach, describe, expect, it, vi } from "vitest";
import { setParams } from "../src/spa/app/router.svelte.js";

function stubLocation(href: string): ReturnType<typeof vi.fn> {
  const replaceState = vi.fn();
  vi.stubGlobal("location", new URL(href));
  vi.stubGlobal("history", { replaceState });
  return replaceState;
}

afterEach(() => vi.unstubAllGlobals());

describe("setParams", () => {
  it("sets a key and keeps the others", () => {
    const replace = stubLocation("http://x/app/answer?thread=t1");
    setParams({ sources: "2" });
    expect(replace).toHaveBeenCalledWith({}, "", "/app/answer?thread=t1&sources=2");
  });

  it("removes a key on null and drops the empty query", () => {
    const replace = stubLocation("http://x/app/?mode=ai");
    setParams({ mode: null });
    expect(replace).toHaveBeenCalledWith({}, "", "/app/");
  });
});
