import type { BrowserAPI, BrowserPort } from "../platform/browser-api.js";
import {
  nativeHostName,
  parseNativeResponse,
  type NativeRequest,
} from "../shared/native-contract.js";

interface PendingRequest {
  readonly resolve: (value: unknown) => void;
  readonly reject: (error: Error) => void;
  readonly timeout: ReturnType<typeof setTimeout>;
}

export interface NativeMessenger {
  request<Payload>(request: NativeRequest): Promise<Payload>;
  close(): void;
}

export class NativePortClient implements NativeMessenger {
  readonly #browser: BrowserAPI;
  readonly #pending = new Map<string, PendingRequest>();
  #port: BrowserPort | null = null;

  constructor(browser: BrowserAPI) {
    this.#browser = browser;
  }

  async request<Payload>(request: NativeRequest): Promise<Payload> {
    let lastError: Error | null = null;
    for (let attempt = 0; attempt < 2; attempt += 1) {
      try {
        const response = await this.#requestOnce(request);
        return parseNativeResponse<Payload>(response, request.requestID);
      } catch (error) {
        lastError =
          error instanceof Error
            ? error
            : new Error("NP_EXTENSION_NATIVE_FAILED");
        if (!isRetryable(lastError)) {
          throw lastError;
        }
        this.#resetPort();
      }
    }
    throw lastError ?? new Error("NP_EXTENSION_NATIVE_FAILED");
  }

  close(): void {
    this.#resetPort();
  }

  #requestOnce(request: NativeRequest): Promise<unknown> {
    const port = this.#connection();
    return new Promise((resolve, reject) => {
      const timeout = setTimeout(() => {
        this.#pending.delete(request.requestID);
        reject(new Error("NP_EXTENSION_NATIVE_TIMEOUT"));
      }, 12_000);
      this.#pending.set(request.requestID, {
        resolve,
        reject,
        timeout,
      });
      try {
        port.postMessage(request);
      } catch {
        clearTimeout(timeout);
        this.#pending.delete(request.requestID);
        reject(new Error("NP_EXTENSION_NATIVE_DISCONNECTED"));
      }
    });
  }

  #connection(): BrowserPort {
    if (this.#port) {
      return this.#port;
    }
    const port = this.#browser.runtime.connectNative(nativeHostName);
    port.onMessage.addListener((message) => {
      this.#receive(message);
    });
    port.onDisconnect.addListener(() => {
      const error = new Error("NP_EXTENSION_NATIVE_DISCONNECTED");
      this.#port = null;
      for (const pending of this.#pending.values()) {
        clearTimeout(pending.timeout);
        pending.reject(error);
      }
      this.#pending.clear();
    });
    this.#port = port;
    return port;
  }

  #receive(message: unknown): void {
    if (
      typeof message !== "object" ||
      message === null ||
      !("requestID" in message) ||
      typeof message.requestID !== "string"
    ) {
      return;
    }
    const pending = this.#pending.get(message.requestID);
    if (!pending) {
      return;
    }
    clearTimeout(pending.timeout);
    this.#pending.delete(message.requestID);
    pending.resolve(message);
  }

  #resetPort(): void {
    const port = this.#port;
    this.#port = null;
    if (port) {
      try {
        port.disconnect();
      } catch {
        // 端口已经由浏览器关闭时无需重复处理。
      }
    }
  }
}

function isRetryable(error: Error): boolean {
  return (
    error.message === "NP_EXTENSION_NATIVE_TIMEOUT" ||
    error.message === "NP_EXTENSION_NATIVE_DISCONNECTED"
  );
}
