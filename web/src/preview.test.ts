import { describe, expect, it } from "vitest";
import { createPreviewController } from "./preview";

describe("createPreviewController", () => {
  it("switches preview rendering mode when format tab changes", () => {
    const output = document.createElement("div");
    const error = document.createElement("p");
    const preview = createPreviewController({ output, error });

    preview.showResult({
      format: "text",
      output: "<svg data-kind='text'></svg>"
    });

    expect(output.textContent).toBe("<svg data-kind='text'></svg>");
    expect(output.querySelector("svg")).toBeNull();

    preview.showResult({
      format: "svg",
      output: "<svg data-kind='svg'></svg>"
    });

    expect(output.querySelector("svg")?.getAttribute("data-kind")).toBe("svg");
  });

  it("shows human-readable error panel when worker returns error", () => {
    const output = document.createElement("div");
    const error = document.createElement("p");
    const preview = createPreviewController({ output, error });

    preview.showError("parse error: expected node id");

    expect(error.hidden).toBe(false);
    expect(error.textContent).toContain("parse error: expected node id");
  });
});
