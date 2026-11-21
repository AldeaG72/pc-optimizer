import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { cleanup, fireEvent, render, screen, within } from "@testing-library/svelte";
import Tweaks from "./Tweaks.svelte";
import Apply from "./Apply.svelte";
import { prefs, ui } from "../lib/state.svelte";
import { defaultSettings } from "../lib/backend";
import { clearPending, pending } from "../lib/tweaks.svelte";

const DVR = "Captură în fundal (Game DVR) oprită";

beforeEach(() => {
  Object.assign(prefs, defaultSettings());
  ui.lang = "ro";
  ui.loaded = true;
  ui.page = "tweaks";
  ui.focus = null;
  clearPending();
});
afterEach(cleanup);

describe("Tweaks", () => {
  it("lists the recommended tweaks, ticks one and shows the apply bar", async () => {
    render(Tweaks);
    const sw = await screen.findByRole("switch", { name: DVR });
    expect(sw.getAttribute("aria-checked")).toBe("false");
    expect(screen.queryByRole("region", { name: "Aplică tot" })).toBeNull();
    await fireEvent.click(sw);
    expect(sw.getAttribute("aria-checked")).toBe("true");
    expect(pending.apply).toEqual(["gaming.game-dvr-off"]);
    const bar = screen.getByRole("region", { name: "Aplică tot" });
    expect(within(bar).getByText("1 modificare de făcut")).toBeTruthy();
    expect(screen.getByText("de aplicat")).toBeTruthy();
    await fireEvent.click(within(bar).getByRole("button", { name: "Renunță" }));
    expect(pending.apply).toEqual([]);
  });

  it("'Recommended' ticks the safe tweaks for this PC", async () => {
    render(Tweaks);
    await screen.findByRole("switch", { name: DVR });
    await fireEvent.click(screen.getByRole("button", { name: /^Recomandate$/ }));
    expect(pending.apply).toEqual(["gaming.game-dvr-off"]);
  });

  it("shows the details, with the exact registry change and sources", async () => {
    render(Tweaks);
    await fireEvent.click(await screen.findByRole("button", { name: DVR }));
    const panel = screen.getByRole("complementary");
    expect(within(panel).getByRole("heading", { name: DVR })).toBeTruthy();
    expect(within(panel).getByText("HKCU\\System\\GameConfigStore\\GameDVR_Enabled")).toBeTruthy();
    expect(within(panel).getByText("lipsă → 0")).toBeTruthy();
    expect(within(panel).getByText("Win+Alt+R nu mai funcționează.")).toBeTruthy();
  });

  it("a tweak that does not apply here cannot be switched, and says why", async () => {
    render(Tweaks);
    await screen.findByRole("switch", { name: DVR });
    await fireEvent.click(screen.getByRole("button", { name: /Servicii/ }));
    const sw = screen.getByRole("switch", { name: "Serviciul Fax oprit" });
    expect((sw as HTMLButtonElement).disabled).toBe(true);
    await fireEvent.click(screen.getByRole("button", { name: "Serviciul Fax oprit" }));
    expect(screen.getByText("Serviciul Fax nu există pe acest PC.")).toBeTruthy();
  });

  it("search finds tweaks across categories", async () => {
    render(Tweaks);
    await screen.findByRole("switch", { name: DVR });
    await fireEvent.input(screen.getByRole("searchbox"), { target: { value: "fax" } });
    expect(screen.getByRole("switch", { name: "Serviciul Fax oprit" })).toBeTruthy();
    expect(screen.queryByRole("switch", { name: DVR })).toBeNull();
    await fireEvent.input(screen.getByRole("searchbox"), { target: { value: "zzz" } });
    expect(screen.getByText("Nimic găsit pentru „zzz”.")).toBeTruthy();
  });
});

describe("Apply", () => {
  it("runs the ticked changes and reports each one", async () => {
    render(Tweaks);
    await fireEvent.click(await screen.findByRole("switch", { name: DVR }));
    cleanup();
    ui.page = "apply";
    render(Apply);
    expect(await screen.findByText("Gata: 1 reușită, 0 sărite")).toBeTruthy();
    expect(screen.getByText("Aplicat")).toBeTruthy();
    expect(pending.apply).toEqual([]);
  });
});
