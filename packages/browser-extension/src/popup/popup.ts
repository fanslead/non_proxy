import { getBrowserAPI } from "../platform/browser-api.js";
import { normalizeWebURL } from "../shared/domain.js";
import {
  learningOriginPattern,
  type ExtensionRequest,
  type ExtensionResponse,
} from "../shared/extension-contract.js";
import type { LearningState } from "../background/learning-controller.js";

const browser = getBrowserAPI();
const elements = {
  domain: requireElement("site-domain"),
  note: requireElement("site-note"),
  idle: requireElement("idle-panel"),
  active: requireElement("active-panel"),
  status: requireElement("status-dot"),
  countdown: requireElement("countdown"),
  summary: requireElement("candidate-summary"),
  error: requireElement("error-message"),
  start: requireButton("start-button"),
  stop: requireButton("stop-button"),
};

let activeTabID: number | null = null;
let currentState: LearningState = {
  active: false,
  candidateCount: 0,
};

elements.start.addEventListener("click", () => {
  void startLearning();
});
elements.stop.addEventListener("click", () => {
  void stopLearning();
});
setInterval(renderCountdown, 1_000);
void initialize();

async function initialize(): Promise<void> {
  try {
    const [tab] = await browser.tabs.query({
      active: true,
      currentWindow: true,
    });
    if (!tab || tab.id === undefined) {
      throw new Error("NP_EXTENSION_TAB_INVALID");
    }
    activeTabID = tab.id;
    const domain = normalizeWebURL(tab.url);
    if (!domain) {
      elements.domain.textContent = "此页面不支持";
      elements.note.textContent =
        "请打开普通 HTTP 或 HTTPS 网站后再使用。";
      elements.start.disabled = true;
      return;
    }
    elements.domain.textContent = domain;
    const response = await send({
      type: "getState",
      tabID: tab.id,
    });
    applyResponse(response);
  } catch (error) {
    showError(errorCode(error));
  }
}

async function startLearning(): Promise<void> {
  if (activeTabID === null) {
    return;
  }
  setBusy(true);
  hideError();
  try {
    const granted = await browser.permissions.request({
      origins: [learningOriginPattern],
    });
    if (!granted) {
      throw new Error("NP_EXTENSION_PERMISSION_DENIED");
    }
    applyResponse(
      await send({ type: "start", tabID: activeTabID }),
    );
  } catch (error) {
    await releasePermission();
    showError(errorCode(error));
  } finally {
    setBusy(false);
  }
}

async function stopLearning(): Promise<void> {
  if (activeTabID === null) {
    return;
  }
  setBusy(true);
  hideError();
  try {
    applyResponse(
      await send({ type: "stop", tabID: activeTabID }),
    );
  } catch (error) {
    showError(errorCode(error));
  } finally {
    setBusy(false);
  }
}

async function send(
  request: ExtensionRequest,
): Promise<ExtensionResponse> {
  const value = await browser.runtime.sendMessage(request);
  if (
    typeof value !== "object" ||
    value === null ||
    !("ok" in value) ||
    typeof value.ok !== "boolean"
  ) {
    throw new Error("NP_EXTENSION_RESPONSE_INVALID");
  }
  return value as ExtensionResponse;
}

function applyResponse(response: ExtensionResponse): void {
  if (!response.ok || !response.state) {
    showError(response.error ?? "NP_EXTENSION_OPERATION_FAILED");
    return;
  }
  currentState = response.state;
  renderState();
}

function renderState(): void {
  elements.idle.hidden = currentState.active;
  elements.active.hidden = !currentState.active;
  elements.status.classList.toggle("active", currentState.active);
  elements.summary.textContent =
    currentState.candidateCount === 0
      ? "尚未发现依赖域名。"
      : `已识别 ${currentState.candidateCount} 个域名，结束后可逐项确认。`;
  renderCountdown();
}

function renderCountdown(): void {
  if (!currentState.active || !currentState.expiresAtUnixMilliseconds) {
    return;
  }
  const remaining = Math.max(
    0,
    Math.ceil(
      (currentState.expiresAtUnixMilliseconds - Date.now()) / 1_000,
    ),
  );
  elements.countdown.textContent = `剩余 ${remaining} 秒`;
  if (remaining === 0) {
    currentState = {
      active: false,
      ...(currentState.normalizedSite
        ? { normalizedSite: currentState.normalizedSite }
        : {}),
      candidateCount: currentState.candidateCount,
    };
    renderState();
  }
}

function setBusy(value: boolean): void {
  elements.start.disabled = value;
  elements.stop.disabled = value;
}

function showError(code: string): void {
  elements.error.textContent = errorMessage(code);
  elements.error.hidden = false;
}

function hideError(): void {
  elements.error.hidden = true;
  elements.error.textContent = "";
}

async function releasePermission(): Promise<void> {
  try {
    await browser.permissions.remove({
      origins: [learningOriginPattern],
    });
  } catch {
    // 权限可能已由后台释放。
  }
}

function errorCode(error: unknown): string {
  return error instanceof Error
    ? error.message
    : "NP_EXTENSION_OPERATION_FAILED";
}

function errorMessage(code: string): string {
  const messages: Readonly<Record<string, string>> = {
    NP_EXTENSION_PERMISSION_DENIED: "未获得临时网站访问权限。",
    NP_EXTENSION_PERMISSION_REQUIRED: "需要先允许本次网站识别。",
    NP_EXTENSION_SITE_UNSUPPORTED: "当前页面不支持网站识别。",
    NP_EXTENSION_NATIVE_TIMEOUT: "连接 NonProxy 超时，请确认主程序已安装。",
    NP_EXTENSION_NATIVE_DISCONNECTED:
      "无法连接 NonProxy，请确认后台服务正在运行。",
  };
  return messages[code] ?? "操作没有完成，请稍后重试。";
}

function requireElement(id: string): HTMLElement {
  const element = document.getElementById(id);
  if (!element) {
    throw new Error(`缺少界面元素 ${id}`);
  }
  return element;
}

function requireButton(id: string): HTMLButtonElement {
  const element = requireElement(id);
  if (!(element instanceof HTMLButtonElement)) {
    throw new Error(`界面元素 ${id} 不是按钮`);
  }
  return element;
}
