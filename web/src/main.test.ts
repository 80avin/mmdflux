import { describe, expect, it } from "vitest";
import { renderApp } from "./main";

describe("renderApp", () => {
  it("renders placeholder playground content", () => {
    const root = document.createElement("div");
    renderApp(root);

    expect(root.textContent).toContain("mmdflux playground");
    expect(root.textContent).toContain("Preview");
    expect(root.textContent).toContain("Text");
  });
});
