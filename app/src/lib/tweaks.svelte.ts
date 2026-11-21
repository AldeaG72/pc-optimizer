// The catalog of tweaks and the changes the user has ticked but not applied yet. Shared by
// the Tweaks page and the Apply page.
import { recipes as fetchRecipes, type Category, type Recipe } from "./backend";
import type { Lang } from "./i18n";

export const catalog = $state({
  list: [] as Recipe[],
  laptop: false,
  loading: false,
  error: null as string | null,
});

/** Ticked, not applied yet: recipes to apply and recipes to undo. */
export const pending = $state({ apply: [] as string[], undo: [] as string[] });

export async function loadCatalog(lang: Lang): Promise<void> {
  catalog.loading = true;
  catalog.error = null;
  try {
    const c = await fetchRecipes(lang);
    catalog.list = c.recipes;
    catalog.laptop = c.laptop;
    // Drop ticks that no longer make sense (state changed meanwhile).
    pending.apply = pending.apply.filter((id) => canApply(byId(id)));
    pending.undo = pending.undo.filter((id) => byId(id)?.status.code === "applied" || byId(id)?.status.code === "modified_outside");
  } catch (e) {
    catalog.error = String(e);
  } finally {
    catalog.loading = false;
  }
}

export function byId(id: string): Recipe | undefined {
  return catalog.list.find((r) => r.id === id);
}

/** Applied by the app (even if changed outside since). */
export function isApplied(r: Recipe): boolean {
  return r.status.code === "applied" || r.status.code === "modified_outside";
}

/** Something to change: not applied, or changed back outside the app. */
export function canApply(r: Recipe | undefined): boolean {
  return !!r && (r.status.code === "not_applied" || r.status.code === "modified_outside");
}

/** Where the switch stands: the recipe's state with the user's ticks on top. */
export function switchOn(r: Recipe): boolean {
  if (pending.undo.includes(r.id)) return false;
  if (pending.apply.includes(r.id)) return true;
  return isApplied(r) || r.status.code === "already_in_place";
}

/** The switch can be used (not for recipes that do not apply or are already in place). */
export function switchEnabled(r: Recipe): boolean {
  return r.status.code !== "not_applicable" && r.status.code !== "already_in_place" && r.status.code !== "error";
}

const without = (list: string[], id: string) => list.filter((x) => x !== id);

export function toggle(r: Recipe): void {
  if (!switchEnabled(r)) return;
  if (pending.apply.includes(r.id)) pending.apply = without(pending.apply, r.id);
  else if (pending.undo.includes(r.id)) pending.undo = without(pending.undo, r.id);
  else if (isApplied(r)) pending.undo = [...pending.undo, r.id];
  else pending.apply = [...pending.apply, r.id];
}

/** "Apply again" for a recipe changed outside the app. */
export function reapply(r: Recipe): void {
  pending.undo = without(pending.undo, r.id);
  if (!pending.apply.includes(r.id)) pending.apply = [...pending.apply, r.id];
}

/** Safe recipes for this PC's profile (Laptop on laptops), not applied yet. */
export function recommended(list: Recipe[], laptop: boolean): Recipe[] {
  const profile = laptop ? "laptop" : "recommended";
  return list.filter((r) => r.risk === "safe" && r.profiles.includes(profile));
}

/** Ticks every recommended recipe of `category` (all categories when null). */
export function tickRecommended(category: Category | null): void {
  for (const r of recommended(catalog.list, catalog.laptop)) {
    if (category && r.category !== category) continue;
    if (r.status.code === "not_applied" && !pending.apply.includes(r.id)) {
      pending.apply = [...pending.apply, r.id];
    }
  }
}

export function clearPending(): void {
  pending.apply = [];
  pending.undo = [];
}

export const pendingCount = (): number => pending.apply.length + pending.undo.length;
