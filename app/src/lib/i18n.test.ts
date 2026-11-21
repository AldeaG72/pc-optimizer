import { describe, expect, it } from "vitest";
import { formatText, Texts } from "./i18n";

// The same cases as crates/core/src/i18n.rs, so `pco` and the app never disagree.
describe("plurals", () => {
  const ro = (n: number) => formatText("ro", "{n} {n|rulare|rulări|de rulări}", { n });

  it("Romanian", () => {
    expect(ro(0)).toBe("0 rulări");
    expect(ro(1)).toBe("1 rulare");
    expect(ro(2)).toBe("2 rulări");
    expect(ro(19)).toBe("19 rulări");
    expect(ro(20)).toBe("20 de rulări");
    expect(ro(100)).toBe("100 de rulări");
    expect(ro(101)).toBe("101 rulări");
    expect(ro(119)).toBe("119 rulări");
    expect(ro(120)).toBe("120 de rulări");
  });

  it("English and placeholders", () => {
    const en = (n: number) => formatText("en", "{n} {n|run|runs} for {game}", { n, game: "cs2" });
    expect(en(1)).toBe("1 run for cs2");
    expect(en(0)).toBe("0 runs for cs2");
    expect(en(3)).toBe("3 runs for cs2");
    expect(formatText("en", "{x} {", {})).toBe("{x} {");
  });

  it("missing keys show the key", () => {
    const t = new Texts("ro", { "app.a": "A {n|unu|câțiva|mulți}" });
    expect(t.t("app.a", { n: 5 })).toBe("A câțiva");
    expect(t.t("app.missing")).toBe("app.missing");
  });
});
