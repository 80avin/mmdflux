import type { WorkerOutputFormat } from "./worker-protocol";

export interface LiveUpdateRequest {
  input: string;
  format: WorkerOutputFormat;
  configJson: string;
}

export interface LiveUpdateRenderRequest extends LiveUpdateRequest {
  seq: number;
}

export interface LiveUpdateRenderResult {
  seq: number;
  format: WorkerOutputFormat;
  output: string;
}

interface LiveUpdateControllerOptions {
  debounceMs?: number;
  render: (request: LiveUpdateRenderRequest) => Promise<LiveUpdateRenderResult>;
  onResult: (result: LiveUpdateRenderResult) => void;
  onError: (message: string) => void;
}

export interface LiveUpdateController {
  schedule: (request: LiveUpdateRequest) => void;
  cancel: () => void;
}

function toMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

export function createLiveUpdateController(
  options: LiveUpdateControllerOptions
): LiveUpdateController {
  const debounceMs = options.debounceMs ?? 200;
  let nextSeq = 0;
  let latestScheduled: LiveUpdateRequest | null = null;
  let latestSeq = 0;
  let timeoutHandle: ReturnType<typeof setTimeout> | null = null;

  const triggerRender = (request: LiveUpdateRequest): void => {
    const seq = ++nextSeq;
    latestSeq = seq;

    void options
      .render({
        seq,
        input: request.input,
        format: request.format,
        configJson: request.configJson
      })
      .then((result) => {
        if (result.seq !== latestSeq) {
          return;
        }
        options.onResult(result);
      })
      .catch((error) => {
        if (seq !== latestSeq) {
          return;
        }
        options.onError(toMessage(error));
      });
  };

  return {
    schedule: (request: LiveUpdateRequest) => {
      latestScheduled = request;

      if (timeoutHandle !== null) {
        clearTimeout(timeoutHandle);
      }

      timeoutHandle = setTimeout(() => {
        timeoutHandle = null;

        if (!latestScheduled) {
          return;
        }

        triggerRender(latestScheduled);
      }, debounceMs);
    },
    cancel: () => {
      if (timeoutHandle !== null) {
        clearTimeout(timeoutHandle);
        timeoutHandle = null;
      }
    }
  };
}
