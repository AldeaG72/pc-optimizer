# Etapa 3 — Aplicația cu ferestre, helper-ul de administrator, catalogul, installer-ul → prima beta

**Scop:** tot ce face azi `pco` în linie de comandă devine o aplicație Windows 11 normală (Tauri 2 + Svelte):
tur la prima pornire, Acasă, Tweak-uri, Aplicare, Setări. Aplicația rulează **fără** drepturi de administrator;
doar la „Aplică tot” / „Anulează” pornește, cu UAC, un helper mic care acceptă numai ID-uri de rețete.
Catalogul crește de la 6 la rețetele din SPEC 4.2 (fără Avansat). La final: installer, dezinstalare care anulează
totul și prima beta pentru grupul selectat (SPEC 6; deciziile din 2026-11-21 sunt în SPEC).

Etapa are patru părți. Fiecare se termină cu CI verde și se unește separat în `main`, cu acordul proprietarului.

## Partea A — Fundația

- [x] **3.1 Designul ecranelor**: cele 5 ecrane din SPEC 3 (Tur, Acasă, Tweak-uri, Aplicare, Setări), luminos și
  întunecat, în română, ca pagini de previzualizare. Proprietarul le aprobă (sau cere schimbări) **înainte** de programare.
  *(Pânza „PC Optimizer – ecrane”, privată, la proprietar: https://claude.ai/artifact/2EyUo45QHBeXLCTaX1GiMc.
  Aprobată fără schimbări pe 2026-11-21. Pentru 3.8, măsurarea pornește cu Ctrl+Shift+F10 în joc (SPEC 4.4).)*
- [x] **3.2 `crates/elevated-helper`** (`pco-helper.exe`):
  - pornit de aplicație cu UAC doar când e nevoie; se închide singur după ce termină;
  - vorbește cu aplicația printr-un canal local (named pipe) pe care îl poate deschide doar utilizatorul curent;
  - acceptă **doar**: „aplică <ID-uri>”, „anulează <ID-uri> | tot” și acțiunile fixe de Depanare (3.14). Orice altceva
    → refuz. Rețetele sunt incluse în helper la compilare, nu citite de pe disc;
  - aceleași gărzi ca `pco` (Windows 11, joc/anti-cheat pornit, jurnal write-ahead, recuperare);
  - dacă UAC îl pornește sub alt cont de administrator decât cel al aplicației: refuză, cu mesaj clar (altfel setările
    „pentru utilizatorul curent” ar ajunge la alt utilizator).
  *(În plus: jurnalul stă în folderul utilizatorului, deci orice program pornit de el l-ar putea modifica; helper-ul
  anulează doar intrări care arată exact spre ținta rețetei incluse pe care o numesc. Celălalt capăt al canalului nu
  poate „împrumuta” drepturile helper-ului. Testat în CI: aplicare → anulare reală prin helper, 2026-11-21.
  Pentru 3.8, helper-ul face și benchmark-ul (PresentMon cere admin, iar un UAC în mijlocul jocului nu e o opțiune):
  primește doar un joc suportat sau un nume simplu de .exe, folosit ca filtru PresentMon, și numere în limite fixe;
  nu schimbă nicio setare.)*
- [x] **3.3 Scheletul aplicației** (`app/`): fereastră ~1100×720 cu bară de titlu proprie, fundal Mica, bară laterală
  pliabilă, temă după Windows, fontul Inter și iconițele Lucide incluse local (fără nicio cerere pe internet).
  Traducerile RO/EN dintr-o singură sursă comună cu `pco`, cu forme de plural corecte („1 rulare”, „3 rulări”).
  *(Traducerile stau acum în `i18n/` la rădăcină. Aplicația are propriul workspace Cargo (`app/src-tauri`), doar
  pentru Windows. Iconiță provizorie. Job nou în CI: svelte-check, vitest, build, fmt, clippy, teste.)*
- [x] **3.4 Legătura cu motorul**: comenzi Tauri subțiri peste `pco-core` — citirile (rețete, plan, sănătate, hardware,
  istoric benchmark) în aplicație; **orice scriere doar prin helper**.
  *(Registrul e citit printr-un `ReadOnlyRegistry` care refuză orice scriere; serviciile prin `read_only()`.
  `run_helper` pornește helper-ul cu UAC și trimite evenimentele live interfeței.)*

## Partea B — Ecranele

- [x] **3.5 Turul de la prima pornire**: 4 ecrane (limbă; ce face / ce nu face; totul se poate anula; recomandarea de
  benchmark + declinarea răspunderii cu „Sunt de acord”, păstrată și în Despre).
- [x] **3.6 Acasă**: hardware-ul detectat (listă compactă), problemele de sănătate ca și carduri cu „Rezolvă” (deschide
  rețeta potrivită sau explicația), rezumatul tweak-urilor, alegerea monitorului pe care joci (când sunt mai multe).
- [x] **3.7 Tweak-uri**: categorii, căutare, nivel de risc, impact estimat, ce poate strica, panou lateral cu detaliile
  tehnice și sursele, comutator albastru când e aplicat, eticheta „modificat în afara aplicației” (și întrebarea
  „Le reaplic?” după un update Windows care le-a resetat). Butoanele „Recomandate” (pe categorie și global), cu profil
  Desktop / Laptop după tipul PC-ului.
- [x] **3.8 Aplicare**: bifele se adună → „Aplică tot” (UAC doar acum) → listă live cu bifă/eroare pe fiecare tweak →
  erorile la final → restart propus dacă e nevoie. Benchmark-ul „Înainte / După” în același flux (jocurile detectate
  + adăugare manuală a unui .exe), cu propunerea de anulare dacă FPS-ul scade.
- [x] **3.9 Setări**: limbă, temă, „Afișează tweak-urile avansate” (avertisment + confirmare), punct de restaurare
  opțional înainte de aplicare, istoricul benchmark (listă + ștergere), „Anulează tot”, Despre (licențe, PresentMon,
  linkul spre `CE-NU-FACEM.md`).
  *(Partea B, 2026-11-21: setările stau în `settings.json` lângă jurnal. Benchmark-ul din aplicație: jocul pornit prin
  launcher, apoi helper-ul măsoară, fiecare rulare pornită cu Ctrl+Shift+F10; testat cap-coadă în CI cu tasta simulată.
  Punctul de restaurare: creat de helper înainte de prima schimbare; dacă Windows nu-l face, tweak-urile se aplică oricum
  și motivul se afișează. Copia zilnică și Depanarea rămân pentru 3.13–3.14.)*

## Partea C — Catalogul de tweak-uri

- [ ] **3.10 Tipuri noi de acțiuni în motor**, fiecare cu undo exact și teste: planul și setările de alimentare
  (`powercfg`), proprietățile plăcii de rețea, aplicațiile preinstalate (eliminare + reinstalare prin comutator —
  se verifică întâi că reinstalarea chiar merge; altfel rămâne doar ascundere), curățarea de fișiere (**nu** se poate
  anula: confirmare înainte, raport cu spațiul eliberat).
- [ ] **3.11 Rețetele** din SPEC 4.2, în afară de Avansat: alimentare (cu profil Laptop), gaming, input, Windows,
  rețea, sunet (doar recomandare), debloat, confidențialitate, curățare. Fiecare cu `why`, `breaks`, `risk`, `sources`
  verificate (ca în `docs/VERIFICARE-RETETE.md`) și cu testul apply → undo → „registrul identic” în CI. Cele marcate
  în SPEC „după benchmark” (planul de alimentare, HAGS) cer întâi o măsurare.
- [ ] **3.12 Aplicațiile la pornire**: lista completă, impactul, ce face fiecare, recomandare; dezactivarea folosește
  același mecanism ca Task Manager-ul, deci se poate anula.

## Partea D — Lansarea beta

- [ ] **3.13 Siguranță**: export de loguri anonimizat (hardware, Windows, tweak-uri + erori, benchmark — fără nume de
  utilizator/PC/IP), backup zilnic opțional al cheilor de registru atinse, mesaj simplu dacă antivirusul blochează.
- [ ] **3.14 Depanare** (în Setări, SPEC 4.8): resetare rețea, reparare Windows Update, scanare sistem (sfc + DISM),
  curățarea cache-ului de shadere, scurtături spre panourile Windows. Acțiuni fixe, rulate de helper.
- [ ] **3.15 Installer** (Setup.exe, NSIS din Tauri) cu helper-ul și PresentMon (amprenta verificată la build).
  **Dezinstalarea anulează întâi toate modificările**, apoi șterge aplicația. Testat în CI: instalare → aplicare
  „Recomandate” → dezinstalare → registrul identic cu înainte.
- [ ] **3.16 Semnarea**: tu faci cererea la SignPath Foundation (îți dau pașii); când e aprobată, CI semnează
  automat. Până atunci, installer nesemnat, cu explicația avertismentului „Windows protected your PC” în README.
- [ ] **3.17 Prima beta**: pre-release pe GitHub Releases, README cu capturi, instrucțiuni pentru beta-testeri.

## Teste

- interfața: teste automate pe componente și pe fiecare ecran, cu motorul simulat (merg și pe Linux), plus capturi
  de ecran luminos/întunecat salvate de CI la fiecare rulare;
- helper-ul: refuză orice mesaj care nu e un ID cunoscut, alt utilizator, alt cont; test real în CI pe Windows
  (aplicație → helper → aplicare → anulare → registrul identic);
- fiecare rețetă nouă: apply → undo în CI, ca până acum;
- **testul proprietarului**: în **Windows Sandbox** (inclus în Windows 11 Pro; o mașină virtuală de unică folosință,
  nimic nu atinge laptopul): instalezi beta-ul, treci prin tur, aplici „Recomandate”, „Anulează tot”, dezinstalezi.
  Pașii exacți în `docs/TESTARE-ETAPA-3.md`. Pe laptop rulezi aplicația doar ca să te uiți (fără aplicare).

## Criterii de ieșire

1. Proprietarul a aprobat designul (3.1) și a făcut testul din Windows Sandbox fără erori.
2. În CI: instalare → aplicare → dezinstalare lasă registrul identic; toate rețetele trec apply → undo.
3. Helper-ul acceptă doar ID-uri cunoscute (teste pentru fiecare refuz).
4. Aplicația nu face nicio cerere pe internet (verificat automat).
5. Beta publicată ca pre-release pe GitHub (semnată dacă SignPath a aprobat, altfel nesemnată, cu explicație).
6. `cargo clippy`, `cargo test` și testele interfeței curate, CI verde.

## În afara Etapei 3

Etapa 4: modul joc și iconița din tray, notificările, pornirea cu Windows, starea rețelei și cardul „Mod joc” de pe
Acasă, setările automate de driver NVIDIA/AMD, testele de rețea și mouse, recomandările pe joc. Mai târziu: categoria
Avansat, suportul Intel, numele final și logo-ul (până atunci: „PC Optimizer” și o iconiță provizorie).
