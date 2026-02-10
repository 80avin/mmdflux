import { describe, expect, it } from "vitest";

import { isBenchmarkModeEnabled } from "../src/main";

describe("benchmark mode routing", () => {
  it("switches to benchmark mode when URL flag is set", () => {
    expect(
      isBenchmarkModeEnabled(new URL("https://example.test/?benchmark=true")),
    ).toBe(true);
    expect(
      isBenchmarkModeEnabled(new URL("https://example.test/?benchmark=1")),
    ).toBe(true);
  });

  it("keeps standard mode when URL flag is absent or falsey", () => {
    expect(isBenchmarkModeEnabled(new URL("https://example.test/"))).toBe(
      false,
    );
    expect(
      isBenchmarkModeEnabled(new URL("https://example.test/?benchmark=false")),
    ).toBe(false);
  });
});
