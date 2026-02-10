import "../styles/main.css";

import { createEditorController } from "./editor";
import {
  DEFAULT_EXAMPLE_ID,
  findExampleById,
  PLAYGROUND_EXAMPLES
} from "./examples";
import { createLiveUpdateController } from "./live-update";
import { createPreviewController } from "./preview";
import { decodeShareState, encodeShareState } from "./share";
import { createThemeController, type ThemePreference } from "./theme";
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
type StateStorage = Pick<Storage, "getItem" | "setItem">;

interface PersistedPlaygroundState {
  v: 1;
  input: string;
  format: PlaygroundFormat;
}

const PLAYGROUND_STATE_STORAGE_KEY = "mmdflux-playground-state";

export interface RenderAppOptions {
  renderClientFactory?: () => RenderWorkerClient | null;
  debounceMs?: number;
  stateStorage?: StateStorage;
}

function resolveStateStorage(explicitStorage?: StateStorage): StateStorage | undefined {
  if (explicitStorage) {
    return explicitStorage;
  }

  try {
    return window.localStorage;
  } catch {
    return undefined;
  }
}

function parsePersistedPlaygroundState(
  rawValue: string | null
): PersistedPlaygroundState | null {
  if (!rawValue) {
    return null;
  }

  try {
    const parsed = JSON.parse(rawValue) as Partial<PersistedPlaygroundState>;
    if (parsed.v !== 1) {
      return null;
    }
    if (typeof parsed.input !== "string") {
      return null;
    }
    if (typeof parsed.format !== "string" || !isPlaygroundFormat(parsed.format)) {
      return null;
    }

    return {
      v: 1,
      input: parsed.input,
      format: parsed.format
    };
  } catch {
    return null;
  }
}

function readPersistedPlaygroundState(
  storage: StateStorage | undefined
): PersistedPlaygroundState | null {
  if (!storage) {
    return null;
  }

  return parsePersistedPlaygroundState(
    storage.getItem(PLAYGROUND_STATE_STORAGE_KEY)
  );
}

function persistPlaygroundState(
  storage: StateStorage | undefined,
  state: PersistedPlaygroundState
): void {
  if (!storage) {
    return;
  }

  storage.setItem(PLAYGROUND_STATE_STORAGE_KEY, JSON.stringify(state));
}

function createDefaultWorker(): Worker {
  return new Worker(new URL("./worker.ts", import.meta.url), { type: "module" });
}

function isPlaygroundFormat(value: string): value is PlaygroundFormat {
  return value === "text" || value === "svg" || value === "mmds";
}

function toMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

function nextThemePreference(current: ThemePreference): ThemePreference {
  if (current === "system") {
    return "light";
  }
  if (current === "light") {
    return "dark";
  }
  return "system";
}

function formatThemeLabel(preference: ThemePreference): string {
  if (preference === "system") {
    return "Theme: System";
  }
  if (preference === "light") {
    return "Theme: Light";
  }
  return "Theme: Dark";
}

async function copyToClipboard(text: string): Promise<boolean> {
  if (!navigator.clipboard?.writeText) {
    return false;
  }

  try {
    await navigator.clipboard.writeText(text);
    return true;
  } catch {
    return false;
  }
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

export function renderApp(root: HTMLElement, options: RenderAppOptions = {}): void {
  const stateStorage = resolveStateStorage(options.stateStorage);
  const restoredShareState = decodeShareState(window.location.hash);
  const restoredLocalState = readPersistedPlaygroundState(stateStorage);
  const defaultExample = findExampleById(DEFAULT_EXAMPLE_ID) ?? PLAYGROUND_EXAMPLES[0];
  const initialInput =
    restoredShareState?.input ??
    restoredLocalState?.input ??
    defaultExample?.input ??
    "";
  const initialFormat =
    restoredShareState?.format ?? restoredLocalState?.format ?? "text";

  root.innerHTML = `
    <main class="playground">
      <header class="toolbar">
        <h1>mmdflux Playground</h1>
        <div class="toolbar-actions">
          <label class="example-picker">
            <span>Example</span>
            <select data-example-select></select>
          </label>
          <div class="format-tabs" role="tablist" aria-label="Output format">
            <button type="button" role="tab" data-format="text" aria-selected="true" class="is-active">Text</button>
            <button type="button" role="tab" data-format="svg" aria-selected="false">SVG</button>
            <button type="button" role="tab" data-format="mmds" aria-selected="false">MMDS</button>
          </div>
          <button type="button" class="toolbar-button" data-theme-toggle>Theme: System</button>
          <button type="button" class="toolbar-button" data-share>Copy Share URL</button>
        </div>
      </header>
      <section class="workspace">
        <div class="panel">
          <h2>Input</h2>
          <div data-editor-root></div>
        </div>
        <div class="panel">
          <h2>Preview</h2>
          <p class="share-status" data-share-status hidden></p>
          <p class="preview-error" data-preview-error hidden></p>
          <div class="preview-output" data-preview-output></div>
        </div>
      </section>
    </main>
  `;

  const editorRoot = root.querySelector<HTMLElement>("[data-editor-root]");
  const previewOutput = root.querySelector<HTMLElement>("[data-preview-output]");
  const previewError = root.querySelector<HTMLElement>("[data-preview-error]");
  const shareStatus = root.querySelector<HTMLElement>("[data-share-status]");
  const shareButton = root.querySelector<HTMLButtonElement>("[data-share]");
  const themeToggleButton = root.querySelector<HTMLButtonElement>("[data-theme-toggle]");
  const exampleSelect = root.querySelector<HTMLSelectElement>("[data-example-select]");
  const formatButtons = root.querySelectorAll<HTMLButtonElement>(
    ".format-tabs button[data-format]"
  );

  if (
    !editorRoot ||
    !previewOutput ||
    !previewError ||
    !shareStatus ||
    !shareButton ||
    !themeToggleButton ||
    !exampleSelect
  ) {
    return;
  }

  const preview = createPreviewController({
    output: previewOutput,
    error: previewError
  });
  const editor = createEditorController({
    root: editorRoot,
    initialValue: initialInput
  });

  for (const example of PLAYGROUND_EXAMPLES) {
    const option = document.createElement("option");
    option.value = example.id;
    option.textContent = `${example.name} · ${example.description}`;
    exampleSelect.append(option);
  }

  const matchedExample = PLAYGROUND_EXAMPLES.find(
    (example) => example.input === initialInput
  );
  if (matchedExample) {
    exampleSelect.value = matchedExample.id;
  } else {
    const customOption = document.createElement("option");
    customOption.value = "__custom__";
    customOption.textContent = "Custom from URL";
    exampleSelect.prepend(customOption);
    exampleSelect.value = "__custom__";
  }

  const matchMedia =
    typeof window.matchMedia === "function"
      ? window.matchMedia.bind(window)
      : undefined;
  const themeStorage = (() => {
    try {
      return window.localStorage;
    } catch {
      return undefined;
    }
  })();
  const themeController = createThemeController({
    root: document.documentElement,
    storage: themeStorage,
    matchMedia
  });
  themeController.apply();
  themeToggleButton.textContent = formatThemeLabel(themeController.getPreference());

  let selectedFormat: PlaygroundFormat = initialFormat;
  const workerClient = options.renderClientFactory
    ? options.renderClientFactory()
    : typeof Worker === "undefined"
      ? null
      : createRenderWorkerClient();

  const setFormat = (format: PlaygroundFormat): void => {
    selectedFormat = format;
    for (const button of formatButtons) {
      const active = button.dataset.format === format;
      button.classList.toggle("is-active", active);
      button.setAttribute("aria-selected", String(active));
    }
  };

  const updateShareStatus = (message: string): void => {
    shareStatus.hidden = false;
    shareStatus.textContent = message;
  };

  const persistCurrentState = (): void => {
    persistPlaygroundState(stateStorage, {
      v: 1,
      input: editor.getValue(),
      format: selectedFormat
    });
  };

  if (!workerClient) {
    preview.showError("Web Worker support is unavailable in this environment.");
    return;
  }

  const liveUpdate = createLiveUpdateController({
    debounceMs: options.debounceMs ?? 0,
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
      persistCurrentState();
      scheduleRender();
    });
  }

  exampleSelect.addEventListener("change", () => {
    const nextExample = findExampleById(exampleSelect.value);
    if (!nextExample) {
      return;
    }

    editor.setValue(nextExample.input);
    persistCurrentState();
    scheduleRender();
  });

  themeToggleButton.addEventListener("click", () => {
    const nextPreference = nextThemePreference(themeController.getPreference());
    themeController.setPreference(nextPreference);
    themeToggleButton.textContent = formatThemeLabel(
      themeController.getPreference()
    );
  });

  shareButton.addEventListener("click", () => {
    const shareState = {
      input: editor.getValue(),
      format: selectedFormat
    };
    const hash = encodeShareState(shareState);
    const shareUrl = `${window.location.origin}${window.location.pathname}#${hash}`;

    history.replaceState(null, "", `#${hash}`);

    void copyToClipboard(shareUrl).then((copied) => {
      if (copied) {
        updateShareStatus("Share URL copied to clipboard.");
        return;
      }
      updateShareStatus("Share URL updated in address bar.");
    });
  });

  editor.onChange(() => {
    persistCurrentState();
    scheduleRender();
  });

  setFormat(selectedFormat);
  persistCurrentState();
  scheduleRender();
}

const root = document.querySelector<HTMLElement>("#app");
if (root) {
  renderApp(root);
}
