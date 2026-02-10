// @vitest-environment node

import { describe, expect, it } from "vitest";

import { resolveViteBasePath } from "../vite.config";

describe("resolveViteBasePath", () => {
  it("defaults to root when no Pages-specific env is set", () => {
    expect(resolveViteBasePath({})).toBe("/");
  });

  it("derives a repository base path in GitHub Actions", () => {
    expect(
      resolveViteBasePath({
        GITHUB_ACTIONS: "true",
        GITHUB_REPOSITORY: "mmds/mmdflux"
      })
    ).toBe("/mmdflux/");
  });

  it("prefers explicit VITE_BASE_PATH override", () => {
    expect(
      resolveViteBasePath({
        GITHUB_ACTIONS: "true",
        GITHUB_REPOSITORY: "mmds/mmdflux",
        VITE_BASE_PATH: "preview/site"
      })
    ).toBe("/preview/site/");
  });
});
