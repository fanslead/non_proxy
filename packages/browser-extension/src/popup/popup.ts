import { getBrowserAPI } from "../platform/browser-api.js";
import { normalizeWebURL } from "../shared/domain.js";
import {
  learningOriginPattern,
  type ExtensionRequest,
  type ExtensionResponse,
} from "../shared/extension-contract.js";
import type { LearningState } from "../shared/learning-state.js";
import {
  renderCandidateChoices,
  selectedCandidateDomains,
  setCandidateChoicesBusy,
} from "./candidate-review.js";

const browser = getBrowserAPI();
const elements = {
  domain: requireElement("site-domain"),
  note: requireElement("site-note"),
  idle: requireElement("idle-panel"),
  active: requireElement("active-panel"),
  review: requireElement("review-panel"),
  success: requireElement("success-panel"),
  status: requireElement("status-dot"),
  countdown: requireElement("countdown"),
  summary: requireElement("candidate-summary"),
  candidates: requireElement("candidate-list"),
  selection: requireElement("selection-summary"),
  successTitle: requireElement("success-title"),
  successCopy: requireElement("success-copy"),
  error: requireElement("error-message"),
  start: requireButton("start-button"),
  stop: requireButton("stop-button"),
  confirm: requireButton("confirm-button"),
  discard: requireButton("discard-button"),
  done: requireButton("done-button"),
};

let activeTabID: number | null = null;
let activeTabDomain: string | null = null;
let expiryRefreshPending = false;
let busy = false;
let currentState: LearningState = {
  active: false,
  candidateCount: 0,
};

elements.start.addEventListener("click", () => void startLearning());
elements.stop.addEventListener("click", () => void stopLearning());
elements.confirm.addEventListener("click", () => void confirmSelection());
elements.discard.addEventListener("click", () => void discardReview());
elements.done.addEventListener("click", () => void discardReview());
setInterval(() => void refreshCountdown(), 1_000);
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
    activeTabDomain = normalizeWebURL(tab.url);
    if (!activeTabDomain) {
      elements.domain.textContent = "此页面不支持";
      elements.note.textContent =
        "请打开普通 HTTP 或 HTTPS 网站后再使用。";
      elements.start.disabled = true;
      return;
    }
    elements.domain.textContent = activeTabDomain;
    applyResponse(
      await send({ type: "getState", tabID: activeTabID }),
    );
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
  if (activeTabID !== null) {
    await runAction({ type: "stop", tabID: activeTabID });
  }
}

async function confirmSelection(): Promise<void> {
  if (activeTabID === null) {
    return;
  }
  const selectedDomains = selectedCandidateDomains(elements.candidates);
  await runAction({
    type: "confirm",
    tabID: activeTabID,
    selectedDomains,
  });
}

async function discardReview(): Promise<void> {
  if (activeTabID !== null) {
    await runAction({ type: "discard", tabID: activeTabID });
  }
}

async function runAction(request: ExtensionRequest): Promise<void> {
  if (activeTabID === null) {
    return;
  }
  setBusy(true);
  hideError();
  try {
    applyResponse(await send(request));
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
  const reviewing = currentState.review !== undefined;
  const confirmed = currentState.confirmation !== undefined;
  elements.idle.hidden = currentState.active || reviewing || confirmed;
  elements.active.hidden = !currentState.active;
  elements.review.hidden = !reviewing;
  elements.success.hidden = !confirmed;
  elements.status.classList.toggle("active", currentState.active);
  elements.status.classList.toggle("ready", reviewing || confirmed);
  elements.domain.textContent =
    currentState.normalizedSite ?? activeTabDomain ?? "此页面不支持";

  if (currentState.active) {
    elements.note.textContent =
      "仅处理当前标签页，不读取网页正文和浏览历史。";
    elements.summary.textContent =
      currentState.candidateCount === 0
        ? "尚未发现依赖域名。"
        : `已识别 ${currentState.candidateCount} 个域名，结束后可逐项确认。`;
    renderCountdown();
  } else if (currentState.review) {
    elements.note.textContent =
      "只显示域名和本地分类，不包含网址路径或页面内容。";
    renderCandidateChoices(
      elements.candidates,
      currentState.review.candidates,
      currentState.review.normalizedSite,
      updateSelectionSummary,
    );
    updateSelectionSummary();
  } else if (currentState.confirmation) {
    const confirmation = currentState.confirmation;
    elements.note.textContent = "规则已在本机创建，不会上传浏览记录。";
    elements.successTitle.textContent =
      `已创建 ${confirmation.policyCount} 条直连规则`;
    elements.successCopy.textContent = confirmationMessage(confirmation);
  } else {
    elements.note.textContent =
      "仅处理当前标签页，不读取网页正文和浏览历史。";
  }
  setBusy(busy);
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
}

async function refreshCountdown(): Promise<void> {
  renderCountdown();
  if (
    !currentState.active ||
    !currentState.expiresAtUnixMilliseconds ||
    currentState.expiresAtUnixMilliseconds > Date.now() ||
    activeTabID === null ||
    expiryRefreshPending
  ) {
    return;
  }
  expiryRefreshPending = true;
  try {
    applyResponse(
      await send({ type: "getState", tabID: activeTabID }),
    );
  } catch (error) {
    showError(errorCode(error));
  } finally {
    expiryRefreshPending = false;
  }
}

function updateSelectionSummary(): void {
  const count = selectedCandidateDomains(elements.candidates).length;
  elements.selection.textContent = `已选择 ${count} 个域名`;
  elements.confirm.textContent = `添加 ${count} 条直连规则`;
  elements.confirm.disabled = busy || count === 0;
}

function setBusy(value: boolean): void {
  busy = value;
  elements.start.disabled = value || activeTabDomain === null;
  elements.stop.disabled = value;
  elements.confirm.disabled =
    value || selectedCandidateDomains(elements.candidates).length === 0;
  elements.discard.disabled = value;
  elements.done.disabled = value;
  setCandidateChoicesBusy(elements.candidates, value);
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
    NP_EXTENSION_SELECTION_INVALID:
      "选择已失效，请保留当前网站并重新确认。",
    NP_EXTENSION_REVIEW_PENDING: "请先处理当前网站的候选域名。",
    NP_EXTENSION_REVIEW_NOT_FOUND: "审核状态已失效，请重新识别。",
    NP_EXTENSION_SESSION_ACTIVE: "识别仍在进行，请先结束识别。",
    NP_EXTENSION_MAIN_CANDIDATE_MISSING:
      "未能验证当前网站，请重新识别。",
    NP_LEARNING_SESSION_ALREADY_CONFIRMED:
      "该次识别已确认，请刷新查看结果。",
    NP_STORAGE_SNAPSHOT_PENDING_EXISTS:
      "上一组网络规则仍在同步，请稍后重试。",
    NP_EXTENSION_NATIVE_TIMEOUT:
      "连接 NonProxy 超时，请确认主程序已安装。",
    NP_EXTENSION_NATIVE_DISCONNECTED:
      "无法连接 NonProxy，请确认后台服务正在运行。",
  };
  return (
    messages[code] ??
    (currentState.review
      ? "操作没有完成，勾选已保留，请稍后重试。"
      : "操作没有完成，请稍后重试。")
  );
}

function confirmationMessage(
  confirmation: NonNullable<LearningState["confirmation"]>,
): string {
  const prefix = confirmation.replayed
    ? "已恢复上次确认结果。"
    : "";
  if (confirmation.snapshotState === "pendingAck") {
    return `${prefix}规则快照 v${confirmation.snapshotVersion} 正在同步到网络组件。`;
  }
  if (confirmation.snapshotState === "active") {
    return `${prefix}规则快照 v${confirmation.snapshotVersion} 已生效。`;
  }
  if (confirmation.snapshotState === "superseded") {
    return `${prefix}规则已保存，更新的规则快照已接管。`;
  }
  if (
    confirmation.snapshotState === "rejected" ||
    confirmation.snapshotState === "rolledBack"
  ) {
    return `${prefix}规则已保存，但快照 v${confirmation.snapshotVersion} 未生效，请打开主程序修复。`;
  }
  return `${prefix}规则快照 v${confirmation.snapshotVersion} 已保存，等待同步。`;
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
