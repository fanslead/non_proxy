import { spawnSync } from "node:child_process";

const [host] = process.argv.slice(2);
if (!host) {
  throw new Error("缺少 Native Messaging Host 路径。");
}

const extensionOrigin =
  "chrome-extension://ldiadofihjimpkhchjicmgcfgjlgidha/";

const denied = spawnSync(host, ["chrome-extension://attacker/"], {
  input: Buffer.from([0, 0, 0, 0]),
  env: process.env,
});
if (denied.status === 0 || denied.stdout.length !== 0) {
  throw new Error("Native Host 未拒绝未授权扩展来源。");
}

function exchange(request) {
  const payload = Buffer.from(JSON.stringify(request), "utf8");
  const header = Buffer.alloc(4);
  header.writeUInt32LE(payload.length);
  const result = spawnSync(host, [extensionOrigin], {
    input: Buffer.concat([header, payload]),
    env: process.env,
    maxBuffer: 2 * 1024 * 1024,
  });
  if (result.status !== 0) {
    throw new Error(
      `Native Host 退出码 ${result.status}：${result.stderr.toString("utf8")}`,
    );
  }
  if (result.stdout.length < 4) {
    throw new Error("Native Host 响应缺少长度前缀。");
  }
  const length = result.stdout.readUInt32LE(0);
  if (length !== result.stdout.length - 4) {
    throw new Error("Native Host 响应帧长度不一致。");
  }
  const response = JSON.parse(
    result.stdout.subarray(4).toString("utf8"),
  );
  if (!response.ok) {
    throw new Error(
      `Native Host 拒绝请求：${response.error?.code ?? "unknown"}`,
    );
  }
  if (response.requestID !== request.requestID) {
    throw new Error("Native Host 响应 requestID 不一致。");
  }
  return response.payload;
}

const started = exchange({
  protocolVersion: 1,
  requestID: "smoke-start",
  type: "startLearning",
  payload: {
    normalizedSite: "nonproxy.test",
    browserContextID: "smoke-browser-context",
    durationMilliseconds: 60000,
  },
});
if (!started.sessionID) {
  throw new Error("Native Host 未返回学习会话 ID。");
}

const observed = exchange({
  protocolVersion: 1,
  requestID: "smoke-observe",
  type: "observeLearning",
  payload: {
    sessionID: started.sessionID,
    observationID: "smoke-observation",
    browserContextID: "smoke-browser-context",
    kind: "subresource",
    normalizedDomain: "api.nonproxy.test",
    initiatorDomain: "nonproxy.test",
    resourceType: "fetch",
  },
});
if (
  observed.duplicate ||
  observed.candidate?.kind !== "requiredFirstParty"
) {
  throw new Error("Native Host 学习观测结果不正确。");
}

const listed = exchange({
  protocolVersion: 1,
  requestID: "smoke-list",
  type: "listCandidates",
  payload: { sessionID: started.sessionID },
});
if (
  listed.session?.browserContextID !== "smoke-browser-context" ||
  listed.candidates?.length !== 1
) {
  throw new Error("Native Host 学习候选查询结果不正确。");
}

const stopped = exchange({
  protocolVersion: 1,
  requestID: "smoke-stop",
  type: "stopLearning",
  payload: { sessionID: started.sessionID },
});
if (
  stopped.candidateCount !== 1 ||
  stopped.session?.state !== "stopped"
) {
  throw new Error("Native Host 停止学习结果不正确。");
}

process.stdout.write(
  "Native Messaging 跨语言联调通过：长度前缀、来源校验、能力认证、UDS 与学习生命周期一致。\n",
);
