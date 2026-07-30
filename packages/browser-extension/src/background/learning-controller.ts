import type { WebRequestDetails } from "../platform/browser-api.js";
import { normalizeWebURL, staysWithinSite } from "../shared/domain.js";
import {
  createNativeRequest,
  type CandidateListResult,
  type ObservationKind,
  type ObservationResult,
  type ResourceType,
  type StartLearningResult,
  type StopLearningResult,
} from "../shared/native-contract.js";
import type { NativeMessenger } from "./native-port-client.js";
import {
  LearningSessionStore,
  type ActiveLearningSession,
} from "./session-store.js";

export interface LearningState {
  readonly active: boolean;
  readonly normalizedSite?: string;
  readonly expiresAtUnixMilliseconds?: number;
  readonly candidateCount: number;
}

interface PendingMainFrameRedirect {
  readonly requestID: string;
  readonly normalizedDomain: string;
}

export class LearningController {
  readonly #native: NativeMessenger;
  readonly #store: LearningSessionStore;
  readonly #onSessionCountChanged: (count: number) => void;
  readonly #pendingMainFrameRedirects =
    new Map<number, PendingMainFrameRedirect>();

  constructor(
    native: NativeMessenger,
    store = new LearningSessionStore(),
    onSessionCountChanged: (count: number) => void = () => {},
  ) {
    this.#native = native;
    this.#store = store;
    this.#onSessionCountChanged = onSessionCountChanged;
  }

  get sessionCount(): number {
    return this.#store.size;
  }

  async start(
    tabID: number,
    normalizedSite: string,
  ): Promise<LearningState> {
    const now = Date.now();
    this.expire(now);
    if (this.#store.get(tabID, now)) {
      return this.state(tabID);
    }
    const browserContextID = `ctx-${crypto.randomUUID()}`;
    const started = await this.#native.request<StartLearningResult>(
      createNativeRequest("startLearning", {
        normalizedSite,
        browserContextID,
        durationMilliseconds: 60_000,
      }),
    );
    this.#store.start({
      tabID,
      sessionID: started.sessionID,
      browserContextID,
      normalizedSite,
      expiresAtUnixMilliseconds: started.expiresAtUnixMilliseconds,
    });
    this.#onSessionCountChanged(this.#store.size);
    try {
      await this.#record(
        this.#store.get(tabID, Date.now()),
        "mainFrame",
        normalizedSite,
        undefined,
        "mainFrame",
      );
    } catch (error) {
      this.#store.remove(tabID);
      this.#onSessionCountChanged(this.#store.size);
      await this.#stopRemote(started.sessionID);
      throw error;
    }
    return this.state(tabID);
  }

  async observe(
    details: WebRequestDetails,
    redirect: boolean,
  ): Promise<void> {
    const now = Date.now();
    this.expire(now);
    const session = this.#store.get(details.tabId, now);
    if (!session) {
      return;
    }
    const normalizedDomain = normalizeWebURL(
      redirect ? details.redirectUrl : details.url,
    );
    if (!normalizedDomain) {
      return;
    }
    if (redirect && details.type === "main_frame") {
      this.#pendingMainFrameRedirects.set(details.tabId, {
        requestID: details.requestId,
        normalizedDomain,
      });
    }
    if (
      !redirect &&
      details.type === "main_frame" &&
      !this.#consumeMainFrameRedirect(details, normalizedDomain) &&
      !staysWithinSite(session.normalizedSite, normalizedDomain)
    ) {
      await this.stop(details.tabId);
      return;
    }
    const kind: ObservationKind = redirect
      ? "redirect"
      : details.type === "main_frame"
        ? "mainFrame"
        : "subresource";
    const initiatorDomain = normalizeWebURL(details.initiator);
    await this.#record(
      session,
      kind,
      normalizedDomain,
      initiatorDomain ?? undefined,
      mapResourceType(details.type),
    );
  }

  async refresh(tabID: number): Promise<LearningState> {
    const now = Date.now();
    this.expire(now);
    const session = this.#store.get(tabID, now);
    if (!session) {
      return idleState();
    }
    const result = await this.#native.request<CandidateListResult>(
      createNativeRequest("listCandidates", {
        sessionID: session.sessionID,
      }),
    );
    for (const candidate of result.candidates) {
      this.#store.addCandidate(tabID, candidate.normalizedDomain);
    }
    return this.state(tabID);
  }

  state(tabID: number): LearningState {
    const now = Date.now();
    this.expire(now);
    const session = this.#store.get(tabID, now);
    if (!session) {
      return idleState();
    }
    return {
      active: true,
      normalizedSite: session.normalizedSite,
      expiresAtUnixMilliseconds: session.expiresAtUnixMilliseconds,
      candidateCount: session.candidateDomains.size,
    };
  }

  async stop(tabID: number): Promise<LearningState> {
    this.#pendingMainFrameRedirects.delete(tabID);
    const session = this.#store.remove(tabID);
    if (!session) {
      return idleState();
    }
    try {
      const stopped = await this.#native.request<StopLearningResult>(
        createNativeRequest("stopLearning", {
          sessionID: session.sessionID,
        }),
      );
      return {
        active: false,
        normalizedSite: session.normalizedSite,
        candidateCount: stopped.candidateCount,
      };
    } finally {
      this.#onSessionCountChanged(this.#store.size);
      if (this.#store.size === 0) {
        this.#native.close();
      }
    }
  }

  expire(nowUnixMilliseconds: number): void {
    const removed = this.#store.removeExpired(nowUnixMilliseconds);
    if (removed.length === 0) {
      return;
    }
    for (const session of removed) {
      this.#pendingMainFrameRedirects.delete(session.tabID);
    }
    this.#onSessionCountChanged(this.#store.size);
    if (this.#store.size === 0) {
      this.#native.close();
    }
  }

  async #record(
    session: ActiveLearningSession | null,
    kind: ObservationKind,
    normalizedDomain: string,
    initiatorDomain: string | undefined,
    resourceType: ResourceType,
  ): Promise<void> {
    if (!session) {
      throw new Error("NP_EXTENSION_SESSION_EXPIRED");
    }
    const result = await this.#native.request<ObservationResult>(
      createNativeRequest("observeLearning", {
        sessionID: session.sessionID,
        observationID: `obs-${crypto.randomUUID()}`,
        browserContextID: session.browserContextID,
        kind,
        normalizedDomain,
        ...(initiatorDomain ? { initiatorDomain } : {}),
        resourceType,
      }),
    );
    this.#store.addCandidate(
      session.tabID,
      result.candidate.normalizedDomain,
    );
  }

  async #stopRemote(sessionID: string): Promise<void> {
    try {
      await this.#native.request<StopLearningResult>(
        createNativeRequest("stopLearning", { sessionID }),
      );
    } catch {
      // gatewayd 会按权威截止时间结束孤立会话。
    }
  }

  #consumeMainFrameRedirect(
    details: WebRequestDetails,
    normalizedDomain: string,
  ): boolean {
    const pending = this.#pendingMainFrameRedirects.get(details.tabId);
    this.#pendingMainFrameRedirects.delete(details.tabId);
    return (
      pending?.requestID === details.requestId &&
      pending.normalizedDomain === normalizedDomain
    );
  }
}

function idleState(): LearningState {
  return { active: false, candidateCount: 0 };
}

function mapResourceType(value: string): ResourceType {
  switch (value) {
    case "main_frame":
      return "mainFrame";
    case "sub_frame":
      return "subFrame";
    case "script":
      return "script";
    case "stylesheet":
      return "styleSheet";
    case "image":
      return "image";
    case "font":
      return "font";
    case "media":
      return "media";
    case "xmlhttprequest":
      return "xmlHttpRequest";
    case "websocket":
      return "webSocket";
    default:
      return "other";
  }
}
