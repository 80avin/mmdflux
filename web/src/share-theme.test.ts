import { describe, expect, it } from "vitest";
import { decodeShareState, encodeShareState } from "./share";
import { resolveTheme } from "./theme";

describe("share state", () => {
  it("encodes and decodes share state losslessly", () => {
    const original = {
      input: "graph LR\nA-->B\nB-->C",
      format: "svg" as const,
    };

    const encoded = encodeShareState(original);
    const decoded = decodeShareState(encoded);

    expect(decoded).toEqual(original);
  });
});

describe("theme preference", () => {
  it("applies theme preference and manual override deterministically", () => {
    expect(
      resolveTheme({ preference: "system", systemPrefersDark: true }),
    ).toBe("dark");
    expect(
      resolveTheme({ preference: "system", systemPrefersDark: false }),
    ).toBe("light");
    expect(resolveTheme({ preference: "light", systemPrefersDark: true })).toBe(
      "light",
    );
    expect(resolveTheme({ preference: "dark", systemPrefersDark: false })).toBe(
      "dark",
    );
  });
});
