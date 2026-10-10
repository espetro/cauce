// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// Backend-health transitions for `src/spa/lib/backend.svelte.ts`:
// probe classification (200-ok / 503-degraded / non-JSON 200 / throws)
// and the toast lifecycle on state changes — silent first healthy
// probe, persistent down/degraded toasts, brief recovery confirm.

import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("svelte-sonner", () => ({
  toast: {
    error: vi.fn(),
    warning: vi.fn(),
    success: vi.fn(),
    dismiss: vi.fn(),
  },
}));

import { toast } from "svelte-sonner";
import {
  backend,
  noteApiCall,
  probeBackend,
} from "../src/spa/lib/backend.svelte.js";

function jsonResponse(status: number, body: unknown): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json" },
  });
}

beforeEach(() => {
  vi.clearAllMocks();
  backend.health = "unknown";
  backend.version = "";
  vi.unstubAllGlobals();
});

describe("probeBackend", () => {
  it("200 {status:ok} marks healthy silently on first probe", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => jsonResponse(200, { status: "ok", version: "1.2.3" })),
    );
    expect(await probeBackend()).toBe("ok");
    expect(backend.version).toBe("1.2.3");
    expect(toast.error).not.toHaveBeenCalled();
    expect(toast.success).not.toHaveBeenCalled();
  });

  it("503 marks degraded and toasts a persistent warning", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => jsonResponse(503, { status: "degraded" })),
    );
    expect(await probeBackend()).toBe("degraded");
    expect(toast.warning).toHaveBeenCalledWith(
      expect.any(String),
      expect.objectContaining({ duration: Number.POSITIVE_INFINITY }),
    );
  });

  it("a 200 that is not the health JSON means no backend path — offline", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => new Response("<html>spa fallback</html>", { status: 200 })),
    );
    expect(await probeBackend()).toBe("offline");
    expect(toast.error).toHaveBeenCalled();
  });

  it("a thrown fetch marks offline", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => {
        throw new TypeError("fetch failed");
      }),
    );
    expect(await probeBackend()).toBe("offline");
    expect(toast.error).toHaveBeenCalled();
  });
});

describe("noteApiCall", () => {
  it("a thrown /api call flips to offline and keeps the toast up", () => {
    noteApiCall(null);
    expect(backend.health).toBe("offline");
    expect(toast.error).toHaveBeenCalledWith(
      expect.any(String),
      expect.objectContaining({ id: "backend-health" }),
    );
  });

  it("5xx on an api call marks degraded", () => {
    noteApiCall(new Response("err", { status: 502 }));
    expect(backend.health).toBe("degraded");
    expect(toast.warning).toHaveBeenCalled();
  });

  it("recovery dismisses the parked toast and confirms once", () => {
    noteApiCall(null);
    expect(backend.health).toBe("offline");
    noteApiCall(new Response("ok", { status: 200 }));
    expect(backend.health).toBe("ok");
    expect(toast.dismiss).toHaveBeenCalledWith("backend-health");
    expect(toast.success).toHaveBeenCalledTimes(1);
  });

  it("4xx never flips health — auth errors are not connectivity", () => {
    noteApiCall(new Response("no", { status: 403 }));
    expect(backend.health).toBe("unknown");
    expect(toast.error).not.toHaveBeenCalled();
    expect(toast.warning).not.toHaveBeenCalled();
  });
});
