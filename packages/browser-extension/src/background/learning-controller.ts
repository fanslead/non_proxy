import type { WebRequestDetails } from "../platform/browser-api.js";
import { normalizeWebURL, staysWithinSite } from "../shared/domain.js";
import type { LearningState } from "../shared/learning-state.js";
import {
  createNativeRequest,
  type CandidateListResult,
  type ConfirmLearningResult,
  type ObservationKind,
  type ObservationResult,
  type ResourceType,
  type StartLearningResult,
  type StopLearningResult,
} from "../shared/native-contract.js";
import {
  idleLearningState,
  pendingReviewState,
  resourceType,
  validateCandidateList,
  validateCandidateSelection,
} from "./learning-controller-support.js";
import {
  LearningReviewStore,
  type PendingLearningReview,
} from "./learning-review-store.js";
import type { NativeMessenger } from "./native-port-client.js";
import {
  LearningSessionStore,
  type ActiveLearningSession,
} from "./session-store.js";

interface PendingMainFrameRedirect {
  readonly requestID: string;
  readonly normalizedDomain: string;
}

export class LearningController {
  readonly #native: NativeMessenger;
  readonly #sessions: LearningSessionStore;
  readonly #reviews: LearningReviewStore;
  readonly #onSessionCountChanged: (count: number) => void;
  readonly #pendingMainFrameRedirects =
    new Map<number, PendingMainFrameRedirect>();
  readonly #finalizations = new Map<string, Promise<LearningState>>();

  constructor(
    native: NativeMessenger,
    sessions = new LearningSessionStore(),
    onSessionCountChanged: (count: number) => void = () => {},
    reviews = new LearningReviewStore(),
  ) {
    this.#native = native;
    this.#sessions = sessions;
    this.#reviews = reviews;
    this.#onSessionCountChanged = onSessionCountChanged;
  }

  get sessionCount(): number {
    return this.#sessions.size;
  }

  async start(
    tabID: number,
    normalizedSite: string,
  ): Promise<LearningState> {
    await this.expire(Date.now());
    if (this.#reviews.pending(tabID)) {
      throw new Error("NP_EXTENSION_REVIEW_PENDING");
    }
    if (this.#sessions.get(tabID, Date.now())) {
      return this.state(tabID);
    }
    this.#reviews.clearCompleted(tabID);
    const browserContextID = `ctx-${crypto.randomUUID()}`;
    const started = await this.#native.request<StartLearningResult>(
      createNativeRequest("startLearning", {
        normalizedSite,
        browserContextID,
        durationMilliseconds: 60_000,
      }),
    );
    this.#sessions.start({
      tabID,
      sessionID: started.sessionID,
      browserContextID,
      normalizedSite,
      expiresAtUnixMilliseconds: started.expiresAtUnixMilliseconds,
    });
    this.#notifySessionCount();
    try {
      await this.#record(
        this.#sessions.get(tabID, Date.now()),
        "mainFrame",
        normalizedSite,
        undefined,
        "mainFrame",
      );
    } catch (error) {
      this.#sessions.remove(tabID);
      this.#notifySessionCount();
      await this.#stopRemote(started.sessionID);
      this.#closeIfIdle();
      throw error;
    }
    return this.state(tabID);
  }

  async observe(
    details: WebRequestDetails,
    redirect: boolean,
  ): Promise<void> {
    await this.expire(Date.now());
    const session = this.#sessions.get(details.tabId, Date.now());
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
      resourceType(details.type),
    );
  }

  async refresh(tabID: number): Promise<LearningState> {
    await this.expire(Date.now());
    const pending = this.#reviews.pending(tabID);
    if (pending) {
      return pending.candidates
        ? this.state(tabID)
        : this.#finalize(pending);
    }
    if (this.#reviews.completed(tabID)) {
      return this.state(tabID);
    }
    const session = this.#sessions.get(tabID, Date.now());
    if (!session) {
      return idleLearningState();
    }
    const result = await this.#native.request<CandidateListResult>(
      createNativeRequest("listCandidates", {
        sessionID: session.sessionID,
      }),
    );
    for (const candidate of result.candidates) {
      this.#sessions.addCandidate(
        tabID,
        candidate.normalizedDomain,
      );
    }
    return this.state(tabID);
  }

  state(tabID: number): LearningState {
    const pending = this.#reviews.pending(tabID);
    if (pending?.candidates) {
      return pendingReviewState(pending);
    }
    const completed = this.#reviews.completed(tabID);
    if (completed) {
      return {
        active: false,
        normalizedSite: completed.normalizedSite,
        candidateCount: completed.confirmation.policyCount,
        confirmation: completed.confirmation,
      };
    }
    const session = this.#sessions.get(tabID, Date.now());
    if (!session) {
      return idleLearningState();
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
    const session = this.#sessions.remove(tabID);
    if (session) {
      const pending = this.#reviews.begin(session);
      this.#notifySessionCount();
      return this.#finalize(pending);
    }
    const pending = this.#reviews.pending(tabID);
    if (!pending || pending.candidates) {
      return this.state(tabID);
    }
    return this.#finalize(pending);
  }

  async confirm(
    tabID: number,
    selectedDomains: readonly string[],
  ): Promise<LearningState> {
    let pending = this.#reviews.pending(tabID);
    if (!pending) {
      throw new Error("NP_EXTENSION_REVIEW_NOT_FOUND");
    }
    if (!pending.candidates) {
      await this.#finalize(pending);
      pending = this.#reviews.pending(tabID);
    }
    if (!pending?.candidates) {
      throw new Error("NP_EXTENSION_REVIEW_NOT_READY");
    }
    validateCandidateSelection(
      pending,
      pending.candidates,
      selectedDomains,
    );
    const result = await this.#native.request<ConfirmLearningResult>(
      createNativeRequest("confirmLearning", {
        sessionID: pending.sessionID,
        confirmationID: pending.confirmationID,
        selectedDomains: [...selectedDomains],
      }),
    );
    const completed = this.#reviews.complete(
      tabID,
      pending.confirmationID,
      result,
    );
    if (!completed) {
      throw new Error("NP_EXTENSION_REVIEW_NOT_FOUND");
    }
    this.#closeIfIdle();
    return this.state(tabID);
  }

  async discard(tabID: number): Promise<LearningState> {
    await this.expire(Date.now());
    if (this.#sessions.get(tabID, Date.now())) {
      throw new Error("NP_EXTENSION_SESSION_ACTIVE");
    }
    if (this.#reviews.discard(tabID)) {
      this.#closeIfIdle();
    }
    return idleLearningState();
  }

  async release(tabID: number): Promise<void> {
    this.#pendingMainFrameRedirects.delete(tabID);
    const session = this.#sessions.remove(tabID);
    const discarded = this.#reviews.discard(tabID);
    if (session) {
      this.#notifySessionCount();
      await this.#stopRemote(session.sessionID);
    }
    if (session || discarded) {
      this.#closeIfIdle();
    }
  }

  async expire(nowUnixMilliseconds: number): Promise<void> {
    const removed = this.#sessions.removeExpired(nowUnixMilliseconds);
    if (removed.length === 0) {
      return;
    }
    const pending = removed.map((session) => {
      this.#pendingMainFrameRedirects.delete(session.tabID);
      return this.#reviews.begin(session);
    });
    this.#notifySessionCount();
    await Promise.allSettled(pending.map((review) => this.#finalize(review)));
  }

  async #finalize(
    pending: PendingLearningReview,
  ): Promise<LearningState> {
    const key = `${pending.tabID}:${pending.sessionID}`;
    const current = this.#finalizations.get(key);
    if (current) {
      return current;
    }
    const task = this.#loadReview(pending);
    this.#finalizations.set(key, task);
    try {
      return await task;
    } finally {
      if (this.#finalizations.get(key) === task) {
        this.#finalizations.delete(key);
      }
    }
  }

  async #loadReview(pending: PendingLearningReview): Promise<LearningState> {
    await this.#native.request<StopLearningResult>(
      createNativeRequest("stopLearning", {
        sessionID: pending.sessionID,
      }),
    );
    const result = await this.#native.request<CandidateListResult>(
      createNativeRequest("listCandidates", {
        sessionID: pending.sessionID,
      }),
    );
    validateCandidateList(pending, result);
    const updated = this.#reviews.setCandidates(
      pending.tabID,
      pending.sessionID,
      result.candidates,
    );
    if (!updated) {
      throw new Error("NP_EXTENSION_REVIEW_NOT_FOUND");
    }
    return pendingReviewState(updated);
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
    this.#sessions.addCandidate(
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

  #notifySessionCount(): void {
    this.#onSessionCountChanged(this.#sessions.size);
  }

  #closeIfIdle(): void {
    if (this.#sessions.size === 0 && this.#reviews.pendingCount === 0) {
      this.#native.close();
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
