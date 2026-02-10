import { describe, expect, it } from "vitest";

import {
  createBenchmarkReport,
  createSummaryRows,
  summarizeSamples,
  toBenchmarkReportJson,
} from "../src/benchmark-report";
import { BENCHMARK_SCENARIOS } from "../src/benchmarks/scenarios";

describe("benchmark metrics", () => {
  it("computes median and p95 correctly from run samples", () => {
    const metrics = summarizeSamples([12, 5, 8, 20, 10, 16, 7, 9, 14, 11]);

    expect(metrics.minMs).toBe(5);
    expect(metrics.maxMs).toBe(20);
    expect(metrics.meanMs).toBe(11.2);
    expect(metrics.medianMs).toBe(10.5);
    expect(metrics.p95Ms).toBe(20);
  });

  it("exports benchmark report JSON with scenario/engine metadata", () => {
    const scenario = BENCHMARK_SCENARIOS.find(
      (candidate) => candidate.complexity === "small",
    );
    if (!scenario) {
      throw new Error("expected to find a small benchmark scenario");
    }

    const report = createBenchmarkReport({
      generatedAt: "2026-02-10T00:00:00.000Z",
      metadata: {
        wasmProfile: "release",
      },
      warmupIterations: 2,
      measurementIterations: 5,
      scenarios: [
        {
          scenario,
          engines: [
            {
              engineId: "mmdflux",
              engineLabel: "mmdflux (WASM)",
              samplesMs: [4, 5, 6, 7, 8],
            },
            {
              engineId: "mermaid",
              engineLabel: "mermaid.js",
              samplesMs: [9, 10, 11, 12, 13],
            },
          ],
        },
      ],
    });

    const summaryRows = createSummaryRows(report);
    expect(summaryRows).toHaveLength(2);

    const reportJson = toBenchmarkReportJson(report);
    expect(reportJson).toContain('"schemaVersion": 1');
    expect(reportJson).toContain('"metadata"');
    expect(reportJson).toContain('"wasmProfile": "release"');
    expect(reportJson).toContain(`"scenarioId": "${scenario.id}"`);
    expect(reportJson).toContain('"engineId": "mmdflux"');
    expect(reportJson).toContain('"engineId": "mermaid"');
    expect(reportJson).toContain('"medianMs":');
    expect(reportJson).toContain('"p95Ms":');
  });

  it("covers small, medium, and large benchmark scenarios", () => {
    const complexitySet = new Set(
      BENCHMARK_SCENARIOS.map((scenario) => scenario.complexity),
    );

    expect(complexitySet.has("small")).toBe(true);
    expect(complexitySet.has("medium")).toBe(true);
    expect(complexitySet.has("large")).toBe(true);
  });
});
