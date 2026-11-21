import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/svelte";
import Apply from "../pages/Apply.svelte";
import BenchPanel from "./BenchPanel.svelte";
import { prefs, ui } from "../lib/state.svelte";
import { defaultSettings } from "../lib/backend";
import { clearPending, loadCatalog, pending } from "../lib/tweaks.svelte";
import { sampleBench } from "../lib/sample";

beforeEach(async () => {
  Object.assign(prefs, defaultSettings());
  ui.lang = "ro";
  ui.loaded = true;
  ui.benchFirst = false;
  clearPending();
  sampleBench.before = null;
  sampleBench.after = null;
  sampleBench.cancelled = false;
  await loadCatalog("ro");
});
afterEach(cleanup);

describe("benchmark", () => {
  it("measure before, apply, measure after, then the verdict", async () => {
    pending.apply = ["gaming.game-dvr-off"];
    ui.benchFirst = true;
    ui.page = "apply";
    render(Apply);
    expect(await screen.findByRole("heading", { level: 1, name: "Măsoară FPS-ul înainte" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "Aplică fără măsurare" })).toBeTruthy();
    await fireEvent.click(await screen.findByRole("button", { name: "Pornește măsurarea" }));
    expect(await screen.findByText("Salvat: 3 rulări.")).toBeTruthy();
    expect(screen.getByText("Rularea 1: FPS mediu 140,0, 1% low 84,0")).toBeTruthy();

    await fireEvent.click(screen.getByRole("button", { name: "Aplică acum" }));
    expect(await screen.findByText("Gata: 1 reușită, 0 sărite")).toBeTruthy();
    expect(await screen.findByText("Pasul următor: măsoară după")).toBeTruthy();
    expect(await screen.findByText(/Ai măsurat Counter-Strike 2 înainte/)).toBeTruthy();
    await fireEvent.click(screen.getByRole("button", { name: "Pornește măsurarea" }));
    expect(await screen.findByText("Counter-Strike 2: înainte → după")).toBeTruthy();
    expect(screen.getByText("A ajutat: FPS mediu +7,1%")).toBeTruthy();
  });

  it("offers the undo after a real FPS drop", async () => {
    sampleBench.before = [160, 161, 162];
    let undone: string[] = [];
    render(BenchPanel, { props: { label: "after", target: "cs2", onUndo: (ids: string[]) => (undone = ids) } });
    await fireEvent.click(await screen.findByRole("button", { name: "Pornește măsurarea" }));
    expect(await screen.findByText("FPS-ul a scăzut cu adevărat. Anulez modificările făcute de atunci?")).toBeTruthy();
    await fireEvent.click(screen.getByRole("button", { name: "Anulează modificările" }));
    expect(undone).toEqual(["gaming.game-dvr-off"]);
  });

  it("an executable must be a bare file name", async () => {
    render(BenchPanel, { props: { label: "before" } });
    const select = await screen.findByLabelText("Jocul");
    await fireEvent.change(select, { target: { value: "other" } });
    const input = screen.getByLabelText("Numele programului");
    await fireEvent.input(input, { target: { value: "C:\\Games\\x.exe" } });
    expect(screen.getByText("Scrie doar numele fișierului, terminat în .exe, fără folder.")).toBeTruthy();
    expect((screen.getByRole("button", { name: "Pornește măsurarea" }) as HTMLButtonElement).disabled).toBe(true);
    await fireEvent.input(input, { target: { value: "msedge.exe" } });
    expect((screen.getByRole("button", { name: "Pornește măsurarea" }) as HTMLButtonElement).disabled).toBe(false);
  });
});
