export interface BrowserEvent<Listener extends (...args: never[]) => unknown> {
  addListener(listener: Listener): void;
}

export interface WebRequestEvent {
  addListener(
    listener: (details: WebRequestDetails) => void,
    filter: { urls: string[] },
  ): void;
}

export interface BrowserPort {
  readonly onMessage: BrowserEvent<(message: unknown) => void>;
  readonly onDisconnect: BrowserEvent<() => void>;
  postMessage(message: unknown): void;
  disconnect(): void;
}

export interface BrowserTab {
  readonly id?: number;
  readonly url?: string;
}

export interface WebRequestDetails {
  readonly tabId: number;
  readonly requestId: string;
  readonly url: string;
  readonly initiator?: string;
  readonly type: string;
  readonly redirectUrl?: string;
}

export interface BrowserAPI {
  readonly runtime: {
    readonly lastError?: { readonly message?: string };
    readonly onMessage: BrowserEvent<
      (
        message: unknown,
        sender: unknown,
      ) => unknown | Promise<unknown>
    >;
    connectNative(application: string): BrowserPort;
    sendMessage(message: unknown): Promise<unknown>;
  };
  readonly permissions: {
    request(permission: { origins: string[] }): Promise<boolean>;
    remove(permission: { origins: string[] }): Promise<boolean>;
    contains(permission: { origins: string[] }): Promise<boolean>;
  };
  readonly tabs: {
    readonly onRemoved: BrowserEvent<(tabID: number) => void>;
    query(query: {
      active: boolean;
      currentWindow: boolean;
    }): Promise<BrowserTab[]>;
    get(tabID: number): Promise<BrowserTab>;
  };
  readonly webRequest: {
    readonly onBeforeRequest: WebRequestEvent;
    readonly onBeforeRedirect: WebRequestEvent;
  };
}

type BrowserGlobal = typeof globalThis & {
  readonly browser?: BrowserAPI;
  readonly chrome?: BrowserAPI;
};

export function getBrowserAPI(): BrowserAPI {
  const global = globalThis as BrowserGlobal;
  const api = global.browser ?? global.chrome;
  if (!api) {
    throw new Error("NP_EXTENSION_BROWSER_API_MISSING");
  }
  return api;
}
