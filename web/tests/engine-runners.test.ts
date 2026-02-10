import { describe, expect, it, vi } from "vitest";

import { createBenchmarkEngineRunners } from "../src/benchmarks/engine-runners";

describe("benchmark engine runners", () => {
  it("normalizes runner interface for mmdflux and mermaid engines", async () => {
    const initMmdflux = vi.fn(async () => {});
    const renderMmdflux = vi.fn(
      (input: string, format: string, configJson: string): string =>
        `<svg data-engine="mmdflux" data-input="${input}" data-format="${format}" data-config="${configJson}"></svg>`
    );
    const renderMermaid = vi.fn(
      async (_id: string, input: string): Promise<{ svg: string }> => ({
        svg: `<svg data-engine="mermaid" data-input="${input}"></svg>`
      })
    );

    const runners = await createBenchmarkEngineRunners({
      loadMmdfluxModule: async () => ({
        default: initMmdflux,
        render: renderMmdflux
      }),
      loadMermaidModule: async () => ({
        initialize: vi.fn(),
        render: renderMermaid
      })
    });

    expect(runners.map((runner) => runner.id)).toEqual(["mmdflux", "mermaid"]);

    for (const runner of runners) {
      await runner.warm("graph TD\nA-->B");
      const output = await runner.render("graph TD\nA-->B");
      expect(output).toContain("<svg");
    }

    expect(initMmdflux).toHaveBeenCalledTimes(1);
    expect(renderMmdflux).toHaveBeenCalledWith("graph TD\nA-->B", "svg", "{}");
    expect(renderMermaid).toHaveBeenCalled();
  });
});
