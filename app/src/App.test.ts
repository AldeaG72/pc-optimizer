import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/svelte";
import App from "./App.svelte";
import { ui } from "./lib/state.svelte";
import { defaultSettings, memory } from "./lib/backend";
import { parseTexts } from "./lib/texts";
import roRaw from "../../i18n/ro.yaml?raw";
import enRaw from "../../i18n/en.yaml?raw";

beforeEach(() => {
  memory.settings = { ...defaultSettings(), lang: "ro", tourAccepted: 1 };
  ui.page = "home";
  ui.loaded = false;
});
afterEach(cleanup);

const title = async () => (await screen.findByRole("heading", { level: 1 })).textContent;

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
    expect(await title()).toBe("Acasă");
    await fireEvent.click(screen.getByRole("button", { name: "Tweak-uri" }));
    expect(await title()).toBe("Tweak-uri");
    expect(screen.getByRole("button", { name: "Tweak-uri" }).getAttribute("aria-current")).toBe("page");
    await fireEvent.click(screen.getByRole("button", { name: "Setări" }));
    expect(await title()).toBe("Setări");
  });

  it("collapses the sidebar to icons, still named, and remembers it", async () => {
    render(App);
    await title();
    await fireEvent.click(screen.getByRole("button", { name: "Pliază bara laterală" }));
    expect(screen.queryByText("Acasă", { selector: ".label" })).toBeNull();
    expect(screen.getByRole("button", { name: "Acasă" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "Deschide bara laterală" }).getAttribute("aria-expanded")).toBe("false");
    await new Promise((r) => setTimeout(r, 0));
    expect(memory.settings.sidebarCollapsed).toBe(true);
  });

  it("changes language and theme from Settings, and saves them", async () => {
    render(App);
    await title();
    await fireEvent.click(screen.getByRole("button", { name: "Setări" }));
    await fireEvent.change(screen.getByLabelText("Limba"), { target: { value: "en" } });
    expect(await title()).toBe("Settings");
    expect(document.documentElement.lang).toBe("en");
    await fireEvent.click(screen.getByRole("radio", { name: "Dark" }));
    expect(document.documentElement.dataset.theme).toBe("dark");
    await new Promise((r) => setTimeout(r, 0));
    expect(memory.settings.lang).toBe("en");
    expect(memory.settings.theme).toBe("dark");
    await fireEvent.click(screen.getByRole("radio", { name: "Like Windows" }));
    expect(document.documentElement.dataset.theme).toBeUndefined();
  });
});

describe("first start", () => {
  it("shows the 4-step tour until the disclaimer is accepted", async () => {
    memory.settings = { ...defaultSettings(), lang: "ro", tourAccepted: null };
    render(App);
    expect(await title()).toBe("Bun venit! Alege limba");
    await fireEvent.click(screen.getByRole("radio", { name: "English" }));
    expect(await title()).toBe("Welcome! Choose your language");
    await fireEvent.click(screen.getByRole("radio", { name: "Română" }));
    await fireEvent.click(screen.getByRole("button", { name: "Continuă" }));
    expect(await title()).toBe("Ce face și ce nu face");
    await fireEvent.click(screen.getByRole("button", { name: "Continuă" }));
    expect(await title()).toBe("Totul se poate anula");
    await fireEvent.click(screen.getByRole("button", { name: "Înapoi" }));
    expect(await title()).toBe("Ce face și ce nu face");
    await fireEvent.click(screen.getByRole("button", { name: "Continuă" }));
    await fireEvent.click(screen.getByRole("button", { name: "Continuă" }));
    expect(await title()).toBe("Măsoară înainte să schimbi");
    expect(screen.getByText(/pe propria răspundere/)).toBeTruthy();
    expect(memory.settings.tourAccepted).toBeNull();
    await fireEvent.click(screen.getByRole("button", { name: "Sunt de acord" }));
    expect(await title()).toBe("Acasă");
    expect(memory.settings.tourAccepted).toBe(1);
    expect(memory.settings.lang).toBe("ro");
  });
});
