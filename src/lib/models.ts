/** Shared model catalog for agent spawn. */

export const DEFAULT_MODEL = "grok-4.6";

/** Known selectable models (id = CLI / API model id). */
export const MODEL_OPTIONS: ReadonlyArray<{
  id: string;
  label: string;
  hint?: string;
}> = [
  {
    id: "grok-4.6",
    label: "grok-4.6",
    hint: "默认 · 最新 frontier",
  },
  {
    id: "grok-4.5",
    label: "grok-4.5",
    hint: "上一代",
  },
];

/** Empty prefs.model means the agent uses its archive default / CLI default. */
export function resolveEffectiveModel(prefsModel: string | undefined | null): string {
  const m = (prefsModel ?? "").trim();
  return m || DEFAULT_MODEL;
}

export function isKnownModel(id: string): boolean {
  return MODEL_OPTIONS.some((o) => o.id === id);
}
