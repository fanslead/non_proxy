const maximumDomainLength = 253;

export function normalizeWebURL(value: string | undefined): string | null {
  if (!value) {
    return null;
  }
  try {
    const parsed = new URL(value);
    if (parsed.protocol !== "http:" && parsed.protocol !== "https:") {
      return null;
    }
    return normalizeDomain(parsed.hostname);
  } catch {
    return null;
  }
}

export function normalizeDomain(value: string): string | null {
  const candidate = value.toLowerCase().replace(/\.$/, "");
  if (
    candidate.length === 0 ||
    candidate.length > maximumDomainLength ||
    candidate.includes(":") ||
    isIPv4(candidate)
  ) {
    return null;
  }
  const labels = candidate.split(".");
  if (
    labels.some(
      (label) =>
        label.length === 0 ||
        label.length > 63 ||
        label.startsWith("-") ||
        label.endsWith("-") ||
        !/^[a-z0-9-]+$/.test(label),
    )
  ) {
    return null;
  }
  return candidate;
}

export function staysWithinSite(
  target: string,
  destination: string,
): boolean {
  return (
    target === destination ||
    target.endsWith(`.${destination}`) ||
    destination.endsWith(`.${target}`)
  );
}

function isIPv4(value: string): boolean {
  const parts = value.split(".");
  return (
    parts.length === 4 &&
    parts.every((part) => {
      if (!/^\d{1,3}$/.test(part)) {
        return false;
      }
      const number = Number(part);
      return number >= 0 && number <= 255;
    })
  );
}
