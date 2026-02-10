import { afterEach, describe, expect, it, vi } from "vitest";
import { createLiveUpdateController } from "../src/live-update";

interface Deferred<T> {
  promise: Promise<T>;
  resolve: (value: T) => void;
  reject: (error: unknown) => void;
}

function deferred<T>(): Deferred<T> {
  let resolve!: (value: T) => void;
  let reject!: (error: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

describe("createLiveUpdateController", () => {
  afterEach(() => {
    vi.useRealTimers();
  });

  it("debounces rapid edits and sends only latest render request", async () => {
    vi.useFakeTimers();

    const render = vi.fn(async (request) => ({
      seq: request.seq,
      format: request.format,
      output: request.input
    }));
    const onResult = vi.fn();

    const controller = createLiveUpdateController({
      debounceMs: 200,
      render,
      onResult,
      onError: vi.fn()
    });

    controller.schedule({
      input: "graph TD\nA-->B",
      format: "text",
      configJson: "{}"
    });
    controller.schedule({
      input: "graph TD\nA-->B\nB-->C",
      format: "text",
      configJson: "{}"
    });
    controller.schedule({
      input: "graph TD\nA-->B\nB-->C\nC-->D",
      format: "text",
      configJson: "{}"
    });

    vi.advanceTimersByTime(199);
    expect(render).not.toHaveBeenCalled();

    vi.advanceTimersByTime(1);
    await Promise.resolve();

    expect(render).toHaveBeenCalledTimes(1);
    expect(render).toHaveBeenCalledWith({
      seq: 1,
      input: "graph TD\nA-->B\nB-->C\nC-->D",
      format: "text",
      configJson: "{}"
    });
    expect(onResult).toHaveBeenCalledTimes(1);
  });

  it("ignores stale results when newer seq is pending", async () => {
    vi.useFakeTimers();

    const first = deferred<{ seq: number; format: string; output: string }>();
    const second = deferred<{ seq: number; format: string; output: string }>();
    const render = vi
      .fn()
      .mockImplementationOnce(() => first.promise)
      .mockImplementationOnce(() => second.promise);
    const onResult = vi.fn();

    const controller = createLiveUpdateController({
      debounceMs: 200,
      render,
      onResult,
      onError: vi.fn()
    });

    controller.schedule({
      input: "graph TD\nA-->B",
      format: "text",
      configJson: "{}"
    });
    vi.advanceTimersByTime(200);

    controller.schedule({
      input: "graph TD\nA-->B\nB-->C",
      format: "text",
      configJson: "{}"
    });
    vi.advanceTimersByTime(200);

    first.resolve({
      seq: 1,
      format: "text",
      output: "stale result"
    });
    await Promise.resolve();
    expect(onResult).not.toHaveBeenCalled();

    second.resolve({
      seq: 2,
      format: "text",
      output: "latest result"
    });
    await Promise.resolve();

    expect(onResult).toHaveBeenCalledTimes(1);
    expect(onResult).toHaveBeenCalledWith({
      seq: 2,
      format: "text",
      output: "latest result"
    });
  });
});
