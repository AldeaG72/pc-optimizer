import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/svelte";
import { flushSync } from "svelte";
import App from "./App.svelte";
import { ui } from "./lib/state.svelte";
import { parseTexts } from "./lib/texts";
import roRaw from "../../i18n/ro.yaml?raw";
import enRaw from "../../i18n/en.yaml?raw";

beforeEach(() => {
  ui.lang = "ro";
  ui.theme = "system";
  ui.page = "home";
  ui.collapsed = false;
});
afterEach(cleanup);

describe("translations", () => {
  it("every line of both files is a flat key: \"text\" line", () => {
    for (const raw of [roRaw, enRaw]) {
      for (const line of raw.split(/\r?\n/)) {
        const ok = line.trim() === "" || line.startsWith("#") || /^[a-z0-9_.]+: ".*"$/.test(line);
        expect(ok, line).toBe(true);
      }
    }
    const ro = parseTexts(roRaw);
    const en = parseTexts(enRaw);
    expect(Object.keys(ro).sort()).toEqual(Object.keys(en).sort());
    expect(ro["app.nav.home"]).toBe("Acasă");
  });
});

describe("window", () => {
  it("shows the three pages in Romanian and switches between them", async () => {
    render(App);
    expect(screen.getByRole("heading", { level: 1 }).textContent).toBe("Acasă");
    await fireEvent.click(screen.getByRole("button", { name: "Tweak-uri" }));
    expect(screen.getByRole("heading", { level: 1 }).textContent).toBe("Tweak-uri");
    expect(screen.getByRole("button", { name: "Tweak-uri" }).getAttribute("aria-current")).toBe("page");
    await fireEvent.click(screen.getByRole("button", { name: "Setări" }));
    expect(screen.getByRole("heading", { level: 1 }).textContent).toBe("Setări");
  });

  it("collapses the sidebar to icons, still named for screen readers", async () => {
    render(App);
    await fireEvent.click(screen.getByRole("button", { name: "Pliază bara laterală" }));
    expect(screen.queryByText("Acasă", { selector: ".label" })).toBeNull();
    expect(screen.getByRole("button", { name: "Acasă" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "Deschide bara laterală" }).getAttribute("aria-expanded")).toBe("false");
  });

  it("changes language and theme from Settings", async () => {
    render(App);
    await fireEvent.click(screen.getByRole("button", { name: "Setări" }));
    await fireEvent.change(screen.getByLabelText("Limba"), { target: { value: "en" } });
    flushSync();
    expect(screen.getByRole("heading", { level: 1 }).textContent).toBe("Settings");
    expect(document.documentElement.lang).toBe("en");
    await fireEvent.click(screen.getByRole("radio", { name: "Dark" }));
    expect(document.documentElement.dataset.theme).toBe("dark");
    await fireEvent.click(screen.getByRole("radio", { name: "Like Windows" }));
    expect(document.documentElement.dataset.theme).toBeUndefined();
  });
});
