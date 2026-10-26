// Duration estimator: learns from completed generations/responses and predicts
// how long the next one will take. Data is persisted in localStorage so the
// model improves across app restarts.

export interface ImageGenFeatures {
  model: string;
  vramMode: string;
  mode: "txt2img" | "img2img";
  steps: number;
  width: number;
  height: number;
}

export interface TextGenFeatures {
  model: string;
  reasoning: string;
}

interface TimedRecord {
  rate: number;
  ts: number;
}

interface EstimatorStore {
  image: Record<string, TimedRecord[]>;
  text: Record<string, TimedRecord[]>;
}

const STORAGE_KEY = "locaryn_duration_estimates_v1";
const MAX_RECORDS_PER_KEY = 20;

function loadStore(): EstimatorStore {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return { image: {}, text: {} };
    const parsed = JSON.parse(raw) as EstimatorStore;
    return { image: parsed.image || {}, text: parsed.text || {} };
  } catch {
    return { image: {}, text: {} };
  }
}

function saveStore(store: EstimatorStore) {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(store));
  } catch {
    // localStorage may be full or disabled; ignore silently.
  }
}

function textKey(features: TextGenFeatures): string {
  return `${features.model}::${features.reasoning || "off"}`;
}

function pushRecord(records: TimedRecord[], rate: number): TimedRecord[] {
  const next = [...records, { rate, ts: Date.now() }];
  if (next.length > MAX_RECORDS_PER_KEY) {
    return next.slice(next.length - MAX_RECORDS_PER_KEY);
  }
  return next;
}

// ── Text generation ────────────────────────────────────────────────────────

export function recordTextGenerationDuration(
  features: TextGenFeatures,
  durationMs: number,
  outputLength: number,
) {
  if (outputLength <= 0 || durationMs < 100) return;

  const store = loadStore();
  const key = textKey(features);
  const rate = durationMs / outputLength; // ms per output character
  store.text[key] = pushRecord(store.text[key] || [], rate);
  saveStore(store);
}
