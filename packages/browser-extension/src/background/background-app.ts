import {
  type BrowserAPI,
  type WebRequestDetails,
} from "../platform/browser-api.js";
import { normalizeWebURL } from "../shared/domain.js";
import {
  learningOriginPattern,
  parseExtensionRequest,
  type ExtensionResponse,
} from "../shared/extension-contract.js";
import { LearningController } from "./learning-controller.js";

export class BackgroundApp {
  readonly #browser: BrowserAPI;
  readonly #learning: LearningController;

  constructor(browser: BrowserAPI, learning: LearningController) {
    this.#browser = browser;
    this.#learning = learning;
  }

  install(): void {
    void this.releaseLearningPermission();
    this.#browser.runtime.onMessage.addListener((message) =>
      this.handleMessage(message),
    );
    this.#browser.webRequest.onBeforeRequest.addListener(
      (details) => {
        void this.observe(details, false);
      },
      { urls: [learningOriginPattern] },
    );
    this.#browser.webRequest.onBeforeRedirect.addListener(
      (details) => {
        void this.observe(details, true);
      },
      { urls: [learningOriginPattern] },
    );
    this.#browser.tabs.onRemoved.addListener((tabID) => {
      void this.#learning.stop(tabID).catch(() => {});
    });
    setInterval(() => {
      this.#learning.expire(Date.now());
    }, 1_000);
  }

  async handleMessage(value: unknown): Promise<ExtensionResponse> {
    const request = parseExtensionRequest(value);
    if (!request) {
      return failure("NP_EXTENSION_REQUEST_INVALID");
    }
    try {
      let state;
      switch (request.type) {
        case "getState":
          state = await this.#learning.refresh(request.tabID);
          break;
        case "start":
          state = await this.#start(request.tabID);
          break;
        case "stop":
          state = await this.#learning.stop(request.tabID);
          break;
      }
      if (this.#learning.sessionCount === 0) {
        await this.releaseLearningPermission();
      }
      return success(state);
    } catch (error) {
      if (this.#learning.sessionCount === 0) {
        await this.releaseLearningPermission();
      }
      return failure(errorCode(error));
    }
  }

  async observe(
    details: WebRequestDetails,
    redirect: boolean,
  ): Promise<void> {
    try {
      await this.#learning.observe(details, redirect);
    } catch {
      // 单个网络事件失败不能中断页面加载或扩展后台进程。
    }
  }

  async releaseLearningPermission(): Promise<void> {
    if (this.#learning.sessionCount !== 0) {
      return;
    }
    try {
      await this.#browser.permissions.remove({
        origins: [learningOriginPattern],
      });
    } catch {
      // 浏览器可能已回收权限或正在关闭。
    }
  }

  async #start(tabID: number) {
    const granted = await this.#browser.permissions.contains({
      origins: [learningOriginPattern],
    });
    if (!granted) {
      throw new Error("NP_EXTENSION_PERMISSION_REQUIRED");
    }
    const tab = await this.#browser.tabs.get(tabID);
    if (tab.id !== tabID) {
      throw new Error("NP_EXTENSION_TAB_INVALID");
    }
    const normalizedSite = normalizeWebURL(tab.url);
    if (!normalizedSite) {
      throw new Error("NP_EXTENSION_SITE_UNSUPPORTED");
    }
    return this.#learning.start(tabID, normalizedSite);
  }
}

function success(
  state: Awaited<ReturnType<LearningController["state"]>>,
): ExtensionResponse {
  return { ok: true, state };
}

function failure(error: string): ExtensionResponse {
  return { ok: false, error };
}

function errorCode(error: unknown): string {
  if (error instanceof Error && /^NP_[A-Z0-9_]+$/.test(error.message)) {
    return error.message;
  }
  return "NP_EXTENSION_OPERATION_FAILED";
}
