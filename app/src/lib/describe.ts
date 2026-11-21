// Texts for engine states and "not applicable" reasons, shared by the Tweaks and Apply pages.

import type { NotApplicableReason, StateView } from "./backend";
import { t } from "./state.svelte";

/** Built-in power plans by their fixed GUID. */
const PLANS: Record<string, string> = {
  "381b4222-f694-41f0-9685-ff5bb260df2e": "app.plan.balanced",
  "8c5e7fda-e8bf-4a96-9a85-a6e23a8c635c": "app.plan.high",
  "e9a42b02-d5df-448d-aa00-03f14749eb61": "app.plan.ultimate",
  "a1841308-3541-4fab-bc81-f71556f20b4a": "app.plan.saver",
};

export function planName(guid: string): string {
  const key = PLANS[guid.toLowerCase()];
  return key ? t(key) : guid;
}

export function stateText(s: StateView): string {
  switch (s.kind) {
    case "missing":
      return t("app.detail.missing");
    case "value":
      return s.text;
    case "service":
      return t(`app.service.${s.start}`);
    case "plan":
      return planName(s.guid);
    case "power":
      return t("app.detail.power", { ac: s.ac, dc: s.dc });
    case "app":
      return t(s.installed ? "app.detail.app_installed" : "app.detail.app_removed");
  }
}

export function naText(r: NotApplicableReason): string {
  switch (r.code) {
    case "service_missing":
      return t("app.na.service_missing", { name: r.name });
    case "precondition":
      return t("app.na.precondition", { target: r.target });
    case "platform":
      return t("app.na.platform");
    default:
      return t(`app.na.${r.code}`);
  }
}
