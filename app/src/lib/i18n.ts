// Texts come from the same files as `pco` (i18n/ro.yaml, i18n/en.yaml), sent by the Rust
// side. Formatting matches crates/core/src/i18n.rs: `{name}` placeholders and plural
// choices `{name|one|few|other}` (English: `{name|one|other}`).

export type Lang = "ro" | "en";
export type Args = Record<string, string | number>;

/** CLDR plural category: 0 = one, 1 = few, 2 = other. */
function category(lang: Lang, value: number): number {
  const n = Math.abs(Math.trunc(value));
  if (n === 1) return 0;
  if (lang === "en") return 2;
  // Romanian: few = 0, or 1–19 after the hundreds (2, 19, 101); other = "20 de", "100 de".
  const rest = n % 100;
  return n === 0 || (rest >= 1 && rest <= 19) ? 1 : 2;
}

function pluralForm(lang: Lang, n: number, forms: string[]): string {
  const wanted = category(lang, n);
  const index = wanted === 0 ? 0 : forms.length === 2 ? 1 : Math.min(wanted, forms.length - 1);
  return forms[index] ?? "";
}

export function formatText(lang: Lang, text: string, args: Args = {}): string {
  return text.replace(/\{([^{}]*)\}/g, (whole, inner: string) => {
    const [name = "", ...forms] = inner.split("|");
    if (!(name in args)) return whole;
    const value = args[name];
    if (forms.length === 0) return String(value);
    const n = typeof value === "number" ? value : Number.parseInt(String(value), 10) || 0;
    return pluralForm(lang, n, forms);
  });
}

export class Texts {
  constructor(
    readonly lang: Lang,
    private readonly map: Record<string, string>,
  ) {}

  /** The text for `key` (the key itself if missing), filled in with `args`. */
  t(key: string, args?: Args): string {
    const text = this.map[key] ?? key;
    return args ? formatText(this.lang, text, args) : text;
  }
}
