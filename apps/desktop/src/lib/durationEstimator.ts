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

const STORAGE_KEY = "lochor_duration_estimates_v1";
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

function imageKey(features: ImageGenFeatures): string {
  return `${features.model}::${features.vramMode}::${features.mode}`;
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

function averageRate(records: TimedRecord[]): number | null {
  if (!records.length) return null;
  // Simple average of the last N runs. We intentionally do not weight older
  // runs down: a user's recent hardware / drivers matter more, and the window
  // is already capped to the last 20.
  const sum = records.reduce((acc, r) => acc + r.rate, 0);
  return sum / records.length;
}

// ── Image generation ───────────────────────────────────────────────────────

export function recordImageGenerationDuration(features: ImageGenFeatures, durationMs: number) {
  const pixelCount = features.width * features.height;
  const complexity = features.steps * pixelCount;
  if (complexity <= 0 || durationMs < 500) return;

  const store = loadStore();
  const key = imageKey(features);
  const rate = durationMs / complexity; // ms per pixel-step
  store.image[key] = pushRecord(store.image[key] || [], rate);
  saveStore(store);
}

export function estimateImageGenerationDuration(features: ImageGenFeatures): number | null {
  const pixelCount = features.width * features.height;
  const complexity = features.steps * pixelCount;
  if (complexity <= 0) return null;

  const store = loadStore();
  const key = imageKey(features);
  const records = store.image[key];
  if (!records || records.length === 0) return null;

  const rate = averageRate(records);
  if (rate === null) return null;

  const estimatedMs = rate * complexity;
  return Math.max(1, Math.round(estimatedMs / 1000));
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

export function estimateTextGenerationDuration(
  features: TextGenFeatures,
  expectedOutputLength: number,
): number | null {
  if (expectedOutputLength <= 0) return null;
  const store = loadStore();
  const key = textKey(features);
  const records = store.text[key];
  if (!records || records.length === 0) return null;

  const rate = averageRate(records);
  if (rate === null) return null;

  const estimatedMs = rate * expectedOutputLength;
  return Math.max(1, Math.round(estimatedMs / 1000));
}

// ── Formatting ──────────────────────────────────────────────────────────────

export function formatEstimatedDuration(seconds: number | null): string {
  if (seconds === null) return "—";
  if (seconds < 60) return `~${seconds}s`;
  const m = Math.floor(seconds / 60);
  const s = seconds % 60;
  return `~${m}m${s > 0 ? ` ${s}s` : ""}`;
}
