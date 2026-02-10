// @vitest-environment node

import { describe, expect, it } from "vitest";

import type { BenchmarkReport } from "../src/benchmark-report";
import { evaluateWasmProfileCompatibility } from "../scripts/benchmark-compare";

function makeReport(
  metadata?: BenchmarkReport["metadata"]
): BenchmarkReport {
  return {
    schemaVersion: 1,
    generatedAt: "2026-02-10T00:00:00.000Z",
    metadata,
    warmupIterations: 1,
    measurementIterations: 3,
    scenarios: []
  };
}

describe("evaluateWasmProfileCompatibility", () => {
  it("passes when both reports have the same wasm profile", () => {
    const result = evaluateWasmProfileCompatibility(
      makeReport({ wasmProfile: "dev" }),
      makeReport({ wasmProfile: "dev" })
    );

    expect(result.issue).toBeNull();
    expect(result.warning).toBeNull();
  });

  it("fails when baseline and current profiles differ", () => {
    const result = evaluateWasmProfileCompatibility(
      makeReport({ wasmProfile: "dev" }),
      makeReport({ wasmProfile: "release" })
    );

    expect(result.warning).toBeNull();
    expect(result.issue).toContain("WASM profile mismatch");
    expect(result.issue).toContain("baseline=dev");
    expect(result.issue).toContain("current=release");
  });

  it("fails when only one report includes wasm profile metadata", () => {
    const result = evaluateWasmProfileCompatibility(
      makeReport({ wasmProfile: "release" }),
      makeReport()
    );

    expect(result.warning).toBeNull();
    expect(result.issue).toContain("missing wasm profile metadata");
  });

  it("warns when neither report includes wasm profile metadata", () => {
    const result = evaluateWasmProfileCompatibility(makeReport(), makeReport());

    expect(result.issue).toBeNull();
    expect(result.warning).toContain("cannot verify WASM profile compatibility");
  });
});
