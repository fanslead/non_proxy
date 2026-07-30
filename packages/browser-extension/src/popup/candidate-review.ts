import type { CandidateResult } from "../shared/native-contract.js";

const kindLabels: Readonly<Record<CandidateResult["kind"], string>> = {
  requiredFirstParty: "站内依赖",
  likelyApi: "接口服务",
  likelyAuth: "登录服务",
  likelyCdn: "内容分发",
  thirdParty: "第三方",
  unknown: "待判断",
};

export function renderCandidateChoices(
  container: HTMLElement,
  candidates: readonly CandidateResult[],
  normalizedSite: string,
  onSelectionChanged: () => void,
): void {
  const legend = container.querySelector("legend");
  container.replaceChildren();
  if (legend) {
    container.append(legend);
  }
  for (const candidate of candidates) {
    container.append(
      candidateRow(candidate, normalizedSite, onSelectionChanged),
    );
  }
}

export function selectedCandidateDomains(
  container: HTMLElement,
): readonly string[] {
  return [...container.querySelectorAll<HTMLInputElement>("input")]
    .filter((input) => input.checked)
    .map((input) => input.value);
}

export function setCandidateChoicesBusy(
  container: HTMLElement,
  busy: boolean,
): void {
  for (const input of container.querySelectorAll<HTMLInputElement>(
    "input",
  )) {
    input.disabled = busy || input.dataset.mandatory === "true";
  }
}

function candidateRow(
  candidate: CandidateResult,
  normalizedSite: string,
  onSelectionChanged: () => void,
): HTMLLabelElement {
  const mandatory = candidate.normalizedDomain === normalizedSite;
  const input = document.createElement("input");
  input.type = "checkbox";
  input.name = "candidate";
  input.value = candidate.normalizedDomain;
  input.checked = mandatory || !candidate.requiresConfirmation;
  input.disabled = mandatory;
  input.dataset.mandatory = String(mandatory);
  input.addEventListener("change", onSelectionChanged);

  const route = document.createElement("span");
  route.className = "route-selector";
  route.setAttribute("aria-hidden", "true");

  const domain = document.createElement("span");
  domain.className = "candidate-domain";
  domain.textContent = candidate.normalizedDomain;

  const kind = document.createElement("span");
  kind.className = "candidate-kind";
  kind.textContent = kindLabels[candidate.kind];

  const trust = document.createElement("span");
  trust.className = candidate.requiresConfirmation
    ? "candidate-trust caution"
    : "candidate-trust";
  trust.textContent = mandatory
    ? "当前网站 · 必须"
    : candidate.requiresConfirmation
      ? "需要确认"
      : "建议直连";

  const evidence = document.createElement("span");
  evidence.className = "candidate-evidence";
  const confidence = Math.round(candidate.confidenceMillis / 10);
  evidence.textContent =
    `${confidence}% · ${candidate.evidenceCount} 次证据`;

  const details = document.createElement("span");
  details.className = "candidate-details";
  details.append(domain, kind, trust, evidence);

  const row = document.createElement("label");
  row.className = "candidate-row";
  if (mandatory) {
    row.classList.add("mandatory");
  }
  row.append(input, route, details);
  return row;
}
