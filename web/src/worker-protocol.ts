export type WorkerOutputFormat = "text" | "ascii" | "svg" | "mmds" | "mermaid";

export interface WorkerRequestMessage {
  type: "render";
  seq: number;
  input: string;
  format: WorkerOutputFormat;
  configJson: string;
}

export interface WorkerResultMessage {
  type: "result";
  seq: number;
  format: WorkerOutputFormat;
  output: string;
}

export interface WorkerErrorMessage {
  type: "error";
  seq: number;
  error: string;
}

export type WorkerResponseMessage = WorkerResultMessage | WorkerErrorMessage;
