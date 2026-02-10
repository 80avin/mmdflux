import { describe, expect, it, vi } from "vitest";
import type { RenderWorkerClient } from "../src/main";
import { renderApp } from "../src/main";

interface MemoryStorage {
  getItem: (key: string) => string | null;
  setItem: (key: string, value: string) => void;
}

function createMemoryStorage(
  initialValues: Record<string, string> = {}
): MemoryStorage {
  const values = new Map(Object.entries(initialValues));
  return {
    getItem: (key) => values.get(key) ?? null,
    setItem: (key, value) => {
      values.set(key, value);
    }
  };
}

function createFakeRenderClient() {
  return {
    render: vi.fn(async (request) => ({
      seq: request.seq,
      format: request.format,
      output: `${request.format}:${request.input}`
    })),
    terminate: vi.fn()
  } satisfies RenderWorkerClient;
}

describe("playground state persistence", () => {
  it("restores editor input and format from persisted state", () => {
    const storage = createMemoryStorage({
      "mmdflux-playground-state": JSON.stringify({
        v: 1,
        input: "graph LR\nPersisted-->State",
        format: "svg"
      })
    });
    const root = document.createElement("div");

    renderApp(root, {
      renderClientFactory: () => createFakeRenderClient(),
      stateStorage: storage
    });

    const editorInput = root.querySelector<HTMLTextAreaElement>(".editor-input");
    const activeTab = root.querySelector<HTMLButtonElement>(
      ".format-tabs button.is-active"
    );

    expect(editorInput?.value).toContain("Persisted-->State");
    expect(activeTab?.dataset.format).toBe("svg");
  });

  it("persists latest editor input and selected format on change", () => {
    const storage = createMemoryStorage();
    const root = document.createElement("div");

    renderApp(root, {
      renderClientFactory: () => createFakeRenderClient(),
      stateStorage: storage
    });

    const editorInput = root.querySelector<HTMLTextAreaElement>(".editor-input");
    const mmdsTab = root.querySelector<HTMLButtonElement>(
      '.format-tabs button[data-format="mmds"]'
    );

    expect(editorInput).not.toBeNull();
    expect(mmdsTab).not.toBeNull();

    editorInput!.value = "graph TD\nA-->Saved";
    editorInput!.dispatchEvent(new Event("input"));
    mmdsTab!.click();

    const persisted = JSON.parse(
      storage.getItem("mmdflux-playground-state") ?? "{}"
    ) as { input?: string; format?: string };

    expect(persisted.input).toBe("graph TD\nA-->Saved");
    expect(persisted.format).toBe("mmds");
  });
});
