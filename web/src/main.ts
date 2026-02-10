import type {
  WorkerOutputFormat,
  WorkerRequestMessage,
  WorkerResponseMessage
} from "./worker-protocol";

export interface RenderRequest {
  input: string;
  format: WorkerOutputFormat;
  configJson?: string;
}

export interface RenderResponse {
  seq: number;
  format: WorkerOutputFormat;
  output: string;
}

interface PendingRequest {
  resolve: (response: RenderResponse) => void;
  reject: (error: Error) => void;
}

export interface RenderWorkerClient {
  render: (request: RenderRequest) => Promise<RenderResponse>;
  terminate: () => void;
}

function createDefaultWorker(): Worker {
  return new Worker(new URL("./worker.ts", import.meta.url), { type: "module" });
}

export function createRenderWorkerClient(
  worker: Worker = createDefaultWorker()
): RenderWorkerClient {
  let seq = 0;
  const pending = new Map<number, PendingRequest>();

  worker.onmessage = (event: MessageEvent<WorkerResponseMessage>) => {
    const response = event.data;
    const pendingRequest = pending.get(response.seq);
    if (!pendingRequest) {
      return;
    }

    pending.delete(response.seq);

    if (response.type === "result") {
      pendingRequest.resolve({
        seq: response.seq,
        format: response.format,
        output: response.output
      });
      return;
    }

    pendingRequest.reject(new Error(response.error));
  };

  return {
    render: (request: RenderRequest) => {
      const currentSeq = ++seq;

      return new Promise<RenderResponse>((resolve, reject) => {
        const message: WorkerRequestMessage = {
          type: "render",
          seq: currentSeq,
          input: request.input,
          format: request.format,
          configJson: request.configJson ?? "{}"
        };

        pending.set(currentSeq, { resolve, reject });

        try {
          worker.postMessage(message);
        } catch (error) {
          pending.delete(currentSeq);
          reject(
            new Error(
              `failed to post render request: ${
                error instanceof Error ? error.message : String(error)
              }`
            )
          );
        }
      });
    },
    terminate: () => {
      worker.terminate();
      for (const request of pending.values()) {
        request.reject(new Error("render worker terminated"));
      }
      pending.clear();
    }
  };
}

export function renderApp(root: HTMLElement): void {
  root.innerHTML = `
    <main>
      <h1>mmdflux Playground</h1>
      <p>Worker-backed WASM render transport is active.</p>
      <pre data-testid="preview">Initializing preview...</pre>
    </main>
  `;

  const preview = root.querySelector<HTMLElement>('[data-testid="preview"]');
  if (!preview) {
    return;
  }

  if (typeof Worker === "undefined") {
    preview.textContent = "Web Worker support is unavailable in this environment.";
    return;
  }

  const renderClient = createRenderWorkerClient();
  void renderClient
    .render({
      input: "graph TD\nA-->B",
      format: "text",
      configJson: "{}"
    })
    .then((response) => {
      preview.textContent = response.output;
    })
    .catch((error: unknown) => {
      preview.textContent = `Render error: ${
        error instanceof Error ? error.message : String(error)
      }`;
    });
}

const root = document.querySelector<HTMLElement>("#app");
if (root) {
  renderApp(root);
}
