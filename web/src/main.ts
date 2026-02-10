import "../styles/main.css";

import { createEditorController } from "./editor";
import { createLiveUpdateController } from "./live-update";
import { createPreviewController } from "./preview";
import type {
  WorkerOutputFormat,
  WorkerRequestMessage,
  WorkerResponseMessage
} from "./worker-protocol";

export interface RenderRequest {
  seq: number;
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

type PlaygroundFormat = "text" | "svg" | "mmds";

const DEFAULT_INPUT = `graph TD
A[Start] --> B{Decision}
B -->|Yes| C[Done]
B -->|No| D[Retry]`;

function createDefaultWorker(): Worker {
  return new Worker(new URL("./worker.ts", import.meta.url), { type: "module" });
}

function isPlaygroundFormat(value: string): value is PlaygroundFormat {
  return value === "text" || value === "svg" || value === "mmds";
}

function toMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

export function createRenderWorkerClient(
  worker: Worker = createDefaultWorker()
): RenderWorkerClient {
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
      const currentSeq = request.seq;

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
          reject(new Error(`failed to post render request: ${toMessage(error)}`));
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
    <main class="playground">
      <header class="toolbar">
        <h1>mmdflux Playground</h1>
        <div class="format-tabs" role="tablist" aria-label="Output format">
          <button type="button" role="tab" data-format="text" aria-selected="true" class="is-active">Text</button>
          <button type="button" role="tab" data-format="svg" aria-selected="false">SVG</button>
          <button type="button" role="tab" data-format="mmds" aria-selected="false">MMDS</button>
        </div>
      </header>
      <section class="workspace">
        <div class="panel">
          <h2>Input</h2>
          <div data-editor-root></div>
        </div>
        <div class="panel">
          <h2>Preview</h2>
          <p class="preview-error" data-preview-error hidden></p>
          <div class="preview-output" data-preview-output></div>
        </div>
      </section>
    </main>
  `;

  const editorRoot = root.querySelector<HTMLElement>("[data-editor-root]");
  const previewOutput = root.querySelector<HTMLElement>("[data-preview-output]");
  const previewError = root.querySelector<HTMLElement>("[data-preview-error]");
  const formatButtons = root.querySelectorAll<HTMLButtonElement>(
    ".format-tabs button[data-format]"
  );

  if (!editorRoot || !previewOutput || !previewError) {
    return;
  }

  const preview = createPreviewController({
    output: previewOutput,
    error: previewError
  });
  const editor = createEditorController({
    root: editorRoot,
    initialValue: DEFAULT_INPUT
  });

  let selectedFormat: PlaygroundFormat = "text";
  const workerClient =
    typeof Worker === "undefined" ? null : createRenderWorkerClient();

  const setFormat = (format: PlaygroundFormat): void => {
    selectedFormat = format;
    for (const button of formatButtons) {
      const active = button.dataset.format === format;
      button.classList.toggle("is-active", active);
      button.setAttribute("aria-selected", String(active));
    }
  };

  if (!workerClient) {
    preview.showError("Web Worker support is unavailable in this environment.");
    return;
  }

  const liveUpdate = createLiveUpdateController({
    debounceMs: 200,
    render: (request) => workerClient.render(request),
    onResult: (response) => {
      preview.showResult({
        format: response.format,
        output: response.output
      });
    },
    onError: (message) => {
      preview.showError(message);
    }
  });

  const scheduleRender = (): void => {
    liveUpdate.schedule({
      input: editor.getValue(),
      format: selectedFormat,
      configJson: "{}"
    });
  };

  for (const button of formatButtons) {
    button.addEventListener("click", () => {
      const format = button.dataset.format;
      if (!format || !isPlaygroundFormat(format)) {
        return;
      }

      setFormat(format);
      scheduleRender();
    });
  }

  editor.onChange(() => {
    scheduleRender();
  });

  setFormat(selectedFormat);
  scheduleRender();
}

const root = document.querySelector<HTMLElement>("#app");
if (root) {
  renderApp(root);
}
