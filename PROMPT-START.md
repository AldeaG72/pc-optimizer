# Mesajul de start pentru Claude Code

Copiază tot textul de sub linie și lipește-l ca prim mesaj în Claude Code,
după ce ai pornit `claude` în folderul proiectului.

---

Salut! Construim împreună un optimizator open-source pentru Windows 11, pentru gameri competitivi.
Eu nu sunt programator: tu scrii codul, eu testez și iau deciziile de produs. Vorbește-mi în română, simplu.

Înainte de orice, citește în ordine:
1. `CLAUDE.md` (regulile proiectului, mai ales regulile de siguranță);
2. `docs/SPEC.md` (toate deciziile de produs);
3. `docs/ETAPA-0.md` (ce construim acum);
4. `docs/CE-NU-FACEM.md`;
5. `recipes/schema/recipe.schema.json` și rețetele din `recipes/`.

Apoi:
- Verifică ce e instalat pe PC (Rust/rustup cu toolchain-ul MSVC, Visual Studio Build Tools cu C++, Git, Node.js LTS). Dacă lipsește ceva, spune-mi exact ce să instalez și comanda de copiat.
- Fă-mi un plan scurt pentru Etapa 0, în pași mici, fiecare cu teste. Așteaptă să-l aprob.
- După aprobare, implementează pas cu pas. După fiecare pas: `cargo fmt`, `cargo clippy -- -D warnings`, `cargo test`, apoi un commit.
- Rețetele din `recipes/` au `verified: false`: verifică fiecare cheie de registru în documentația Microsoft înainte să le marchezi `true`, și înlocuiește sursele care nu sunt exacte.
- Nu aplica niciodată tweak-uri reale pe acest PC. E laptopul meu de programare. Testele care modifică registrul folosesc doar `HKCU\Software\PcOptimizerTest`. Testele pe setări reale se fac în CI (`windows-latest`) și în mașina mea virtuală, urmând `docs/TESTARE-ETAPA-0.md`, pe care o scrii tu la finalul etapei.
- Laptopul meu are grafică integrată Intel, fără placă dedicată. Ține cont de asta când îmi ceri să testez ceva.

Dacă ceva din specificație e neclar sau contradictoriu, întreabă-mă înainte să construiești.
