export interface EditorController {
  getValue: () => string;
  setValue: (value: string) => void;
  onChange: (listener: (value: string) => void) => () => void;
}

interface CreateEditorControllerOptions {
  root: HTMLElement;
  initialValue: string;
}

export function createEditorController(
  options: CreateEditorControllerOptions
): EditorController {
  const textarea = document.createElement("textarea");
  textarea.className = "editor-input";
  textarea.value = options.initialValue;
  textarea.setAttribute("aria-label", "Mermaid input");
  options.root.replaceChildren(textarea);

  const listeners = new Set<(value: string) => void>();
  textarea.addEventListener("input", () => {
    for (const listener of listeners) {
      listener(textarea.value);
    }
  });

  return {
    getValue: () => textarea.value,
    setValue: (value: string) => {
      textarea.value = value;
    },
    onChange: (listener) => {
      listeners.add(listener);
      return () => {
        listeners.delete(listener);
      };
    }
  };
}
