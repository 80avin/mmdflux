import { afterEach, describe, expect, it, vi } from "vitest";
import type { RenderWorkerClient } from "../src/main";
import { renderApp } from "../src/main";

function createFakeRenderClient() {
  const render = vi.fn(async (request) => ({
    seq: request.seq,
    format: request.format,
    output: `${request.format}:${request.input}`,
  }));
  return {
    render,
    terminate: vi.fn(),
  } satisfies RenderWorkerClient;
}

describe("playground examples", () => {
  afterEach(() => {
    vi.useRealTimers();
  });

  it("loads selected example into editor and triggers render", async () => {
    vi.useFakeTimers();
    const root = document.createElement("div");
    const renderClient = createFakeRenderClient();

    renderApp(root, {
      renderClientFactory: () => renderClient,
      debounceMs: 50,
    });

    const exampleSelect = root.querySelector<HTMLSelectElement>(
      "[data-example-select]",
    );
    const editorInput =
      root.querySelector<HTMLTextAreaElement>(".editor-input");

    if (!exampleSelect || !editorInput) {
      throw new Error("expected example select and editor input");
    }

    renderClient.render.mockClear();

    exampleSelect.value = "sequence-basics";
    exampleSelect.dispatchEvent(new Event("change"));
    vi.advanceTimersByTime(50);
    await Promise.resolve();

    expect(editorInput.value).toContain("sequenceDiagram");
    expect(renderClient.render).toHaveBeenCalledTimes(1);
    expect(renderClient.render.mock.calls[0]?.[0]).toMatchObject({
      format: "text",
    });
  });

  it("preserves current format while swapping examples", async () => {
    vi.useFakeTimers();
    const root = document.createElement("div");
    const renderClient = createFakeRenderClient();

    renderApp(root, {
      renderClientFactory: () => renderClient,
      debounceMs: 50,
    });

    const svgTab = root.querySelector<HTMLButtonElement>(
      'button[data-format="svg"]',
    );
    const exampleSelect = root.querySelector<HTMLSelectElement>(
      "[data-example-select]",
    );

    if (!svgTab || !exampleSelect) {
      throw new Error("expected svg tab and example select");
    }

    svgTab.click();
    vi.advanceTimersByTime(50);
    await Promise.resolve();

    renderClient.render.mockClear();

    exampleSelect.value = "class-basics";
    exampleSelect.dispatchEvent(new Event("change"));
    vi.advanceTimersByTime(50);
    await Promise.resolve();

    expect(renderClient.render).toHaveBeenCalledTimes(1);
    expect(renderClient.render.mock.calls[0]?.[0]).toMatchObject({
      format: "svg",
    });
  });
});
