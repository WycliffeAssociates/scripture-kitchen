/**
 * Which key a tweak's value is under, and how a value gets written.
 *
 * Three levels, all readable in a URL bar because a designer edits these by
 * hand and pastes them into chat:
 *
 *   * `<namespace>.v`                     which variant is showing
 *   * `<namespace>.<tweak>`               a tweak that applies to every variant
 *   * `<namespace>.<variant>.<tweak>`     a tweak belonging to one variant
 *
 * A value sitting at its declared default is REMOVED rather than written. A
 * link carrying every default is one nobody can see the interesting part of,
 * and the interesting part is the entire reason the link is being sent.
 */

import type { Tweak, Variant } from "./types.ts";

const VARIANT_KEY = "v";

/**
 * A toggle reads "on"/"off" rather than "true"/"false", and that is not taste.
 * A router that serialises search params as JSON — TanStack's does — sees the
 * string "false" as a JSON literal and writes it quoted, so the URL comes out
 * as `?x.y=%22false%22`: correct on the round trip, unreadable in a link, and
 * un-editable by the designer who is supposed to be able to hand-edit these.
 * "on" and "off" are not JSON literals, so they travel bare.
 */
export const ON = "on";
export const OFF = "off";

export const isOn = (value: string): boolean => value === ON;

const defaultOf = (tweak: Tweak): string => {
  if (tweak.kind === "toggle") return tweak.initial === true ? ON : OFF;
  if (typeof tweak.initial === "string") return tweak.initial;
  return tweak.options?.[0] ?? "";
};

const prefixed = (namespace: string, rest: string): string =>
  namespace === "" ? rest : `${namespace}.${rest}`;

const variantKey = (namespace: string): string => prefixed(namespace, VARIANT_KEY);

const tweakKey = (namespace: string, variantId: string | null, key: string): string =>
  prefixed(namespace, variantId === null ? key : `${variantId}.${key}`);

/** The variant showing, defaulting to the first declared one. */
export const currentVariant = (
  namespace: string,
  variants: readonly Variant[],
  state: Readonly<Record<string, string>>,
): Variant | undefined => {
  const asked = state[variantKey(namespace)];
  // An unknown id falls back rather than blanking the page: the commonest way
  // to get one is a link written against a build where that variant existed.
  return variants.find((variant) => variant.id === asked) ?? variants[0];
};

export const readTweak = (
  namespace: string,
  variantId: string | null,
  tweak: Tweak,
  state: Readonly<Record<string, string>>,
): string => state[tweakKey(namespace, variantId, tweak.key)] ?? defaultOf(tweak);

export const withTweak = (
  state: Readonly<Record<string, string>>,
  namespace: string,
  variantId: string | null,
  tweak: Tweak,
  value: string,
): Record<string, string> => {
  const next = { ...state };
  const key = tweakKey(namespace, variantId, tweak.key);
  if (value === defaultOf(tweak)) delete next[key];
  else next[key] = value;
  return next;
};

/**
 * Switching variant drops the outgoing variant's own tweaks and keeps the
 * shared ones. They are namespaced, so leaving them would put keys in the URL
 * that nothing on screen can explain — and the shared tweaks are shared
 * precisely because they should survive the switch.
 */
export const withVariant = (
  state: Readonly<Record<string, string>>,
  namespace: string,
  variants: readonly Variant[],
  id: string,
): Record<string, string> => {
  const next: Record<string, string> = {};
  const stale = new Set(
    variants.flatMap((variant) =>
      (variant.tweaks ?? []).map((tweak) => tweakKey(namespace, variant.id, tweak.key)),
    ),
  );
  for (const [key, value] of Object.entries(state)) {
    if (!stale.has(key)) next[key] = value;
  }
  if (id !== variants[0]?.id) next[variantKey(namespace)] = id;
  else delete next[variantKey(namespace)];
  return next;
};
