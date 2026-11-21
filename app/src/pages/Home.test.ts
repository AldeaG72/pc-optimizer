import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/svelte";
import Home from "./Home.svelte";
import { prefs, ui } from "../lib/state.svelte";
import { defaultSettings, memory } from "../lib/backend";

beforeEach(() => {
  memory.settings = { ...defaultSettings(), lang: "ro", tourAccepted: 1 };
  Object.assign(prefs, defaultSettings());
  ui.lang = "ro";
  ui.loaded = true;
  ui.page = "home";
  ui.focus = null;
});
afterEach(cleanup);

describe("Home", () => {
  it("lists the hardware and the findings for the monitor you play on", async () => {
    render(Home);
    expect(await screen.findByText("Intel Core i5-1245U")).toBeTruthy();
    expect(screen.getByText("10 nuclee, 12 fire")).toBeTruthy();
    // Two monitors: only the main one's findings show at first.
    expect(screen.getByText("Laptop pe baterie")).toBeTruthy();
    expect(screen.queryByText("Frecvența monitorului")).toBeNull();
    await fireEvent.change(screen.getByLabelText("Pe ce monitor joci?"), { target: { value: "Monitor 2" } });
    expect(await screen.findByText("Frecvența monitorului")).toBeTruthy();
    expect(prefs.gameMonitor).toBe("Monitor 2");
    expect(memory.settings.gameMonitor).toBe("Monitor 2");
    expect(screen.getByText("În regulă (2): TRIM, Spațiu liber")).toBeTruthy();
  });

  it("shows how to fix a finding on Details", async () => {
    render(Home);
    await screen.findByText("Laptop pe baterie");
    const details = screen.getAllByRole("button", { name: "Detalii" })[0]!;
    await fireEvent.click(details);
    expect(screen.getByText("Conectează încărcătorul când joci.")).toBeTruthy();
    expect(screen.getByRole("button", { name: "Ascunde" }).getAttribute("aria-expanded")).toBe("true");
  });

  it("counts tweaks and opens the recommendations", async () => {
    render(Home);
    expect(await screen.findByText("1 recomandat pentru acest PC")).toBeTruthy();
    expect(screen.getByText("0 aplicate")).toBeTruthy();
    await fireEvent.click(screen.getByRole("button", { name: "Vezi recomandările" }));
    expect(ui.page).toBe("tweaks");
    expect(ui.focus).toBe("recommended");
  });
});
