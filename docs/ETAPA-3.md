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

- [x] **3.10 Tipuri noi de acțiuni în motor**, fiecare cu undo exact și teste: planul și setările de alimentare
  (`powercfg`), proprietățile plăcii de rețea, aplicațiile preinstalate (eliminare + reinstalare prin comutator —
  se verifică întâi că reinstalarea chiar merge; altfel rămâne doar ascundere), curățarea de fișiere (**nu** se poate
  anula: confirmare înainte, raport cu spațiul eliberat).
  *(2026-11-22: planul și setările de alimentare prin aceeași interfață Windows ca `powercfg`; setarea plăcii de rețea
  pe fiecare placă fizică ce o oferă (o intrare în jurnal pe placă). O aplicație se elimină doar dacă Windows îi
  păstrează o copie, deci reinstalarea merge fără internet; altfel „nu se aplică” (în loc de ascundere). Windows
  Security, Store, Edge și componentele comune nu pot fi eliminate niciodată. Testat în CI pe Windows real: plan,
  setare USB, setare de placă de rețea, Feedback Hub eliminat și reinstalat. Jurnalul a trecut la versiunea 3 (vechile
  intrări se păstrează). Curățarea: locurile sunt fixe în cod, linkurile nu sunt urmate niciodată; fișierele tale le
  curăță aplicația fără drepturi de administrator, iar `C:\Windows\Temp` îl curăță helper-ul, doar fișierele direct
  din folder, ca nimic cu drepturi de administrator să nu lucreze într-un folder controlat de utilizator.)*
- [x] **3.11 Rețetele** din SPEC 4.2, în afară de Avansat: alimentare (cu profil Laptop), gaming, input, Windows,
  rețea, sunet (doar recomandare), debloat, confidențialitate, curățare. Fiecare cu `why`, `breaks`, `risk`, `sources`
  verificate (ca în `docs/VERIFICARE-RETETE.md`) și cu testul apply → undo → „registrul identic” în CI. Cele marcate
  în SPEC „după benchmark” (planul de alimentare, HAGS) cer întâi o măsurare.
  *(2026-11-22: 39 de rețete (erau 6). Planul de alimentare și HAGS nu sunt în „Recomandate”; interfața oferă
  „Măsoară înainte”. Toate rămân `verified: false` până la verificarea pe PC-uri reale de beta-testeri.
  **Lăsate deoparte, cu motiv:** Power Throttling oprit (locul exact din registru nu e confirmat oficial); MPO oprit
  (nicio pagină oficială NVIDIA/Microsoft găsită); Game Bar oprit (Windows 11 nu are o setare documentată, iar
  aplicația Game Bar e protejată); optimizări pentru jocuri în fereastră (valoarea ține și alte setări, de ex. Auto
  HDR; cere o condiție „doar dacă lipsește”); notificări în timpul jocului (le face Modul joc, Etapa 4); „Permite
  computerului să oprească placa de rețea” (Microsoft spune să nu se schimbe din registru); cache-ul web Steam (Valve
  nu are o pagină oficială); cache-ul Windows Update (Microsoft cere oprirea serviciului întâi); OneDrive (doar oprit
  la pornire, în 3.12); sunet (doar recomandare, Etapa 4). Suspendarea USB e inclusă, dar nu în „Recomandate”:
  Microsoft recomandă s-o lași pornită dacă dispozitivele merg bine.)*
- [x] **3.12 Aplicațiile la pornire**: lista completă, impactul, ce face fiecare, recomandare; dezactivarea folosește
  același mecanism ca Task Manager-ul, deci se poate anula.
  *(2026-11-22: categoria „Pornire” din Tweak-uri: registrul Run (utilizator, toți utilizatorii, 32 de biți) și
  folderele Startup. Impactul e o estimare dintr-o listă de programe cunoscute, plus mărimea programului (Windows nu
  dă altor programe impactul măsurat de Task Manager). Oprirea scrie doar comutatorul din `StartupApproved`, ca Task
  Manager; programul rămâne neatins. Trece prin jurnal, deci „Anulează tot” și dezinstalarea le repornesc. Helper-ul
  primește doar ID-uri din propria lui listă și refuză un jurnal care ar ținti altceva decât comutatoarele. Aplicațiile
  din Store: din Setări › Aplicații › Pornire. Testat în CI pe o intrare de probă.)*

## Partea D — Lansarea beta

- [x] **3.13 Siguranță**: export de loguri anonimizat (hardware, Windows, tweak-uri + erori, benchmark — fără nume de
  utilizator/PC/IP), backup zilnic opțional al cheilor de registru atinse, mesaj simplu dacă antivirusul blochează.
  *(2026-11-22: în Setări › Siguranță. Exportul: fereastra „Salvează ca”, `pc-optimizer-log.txt`; numele de
  utilizator, al PC-ului, folderul profilului, orice `\Users\<nume>` și adresele IPv4 sunt înlocuite, etichetele de
  volum lipsesc. Copia zilnică: un `.reg` cu valorile schimbate de aplicație, în `backups\` lângă jurnal, ultimele 7
  zile; se importă cu dublu-clic. Antivirus: dacă helper-ul lipsește, e refuzat de Windows sau oprit înainte să se
  conecteze, aplicația spune clar că probabil antivirusul l-a blocat și ce poți face.)*
- [x] **3.14 Depanare** (în Setări, SPEC 4.8): resetare rețea, reparare Windows Update, scanare sistem (sfc + DISM),
  curățarea cache-ului de shadere, scurtături spre panourile Windows. Acțiuni fixe, rulate de helper.
  *(2026-11-22: pașii sunt scriși în `pco_core::repair` (procedurile Microsoft); helper-ul primește doar numele
  acțiunii. Repararea Windows Update oprește temporar serviciile și le pornește la loc chiar dacă un pas eșuează; setările
  lor de pornire nu se schimbă. Fiecare acțiune cere confirmare; după resetarea rețelei se propune restartul. Cache-ul
  de shadere (DirectX, NVIDIA, AMD) e în folderele utilizatorului, deci îl golește aplicația, fără admin.)*
- [x] **3.15 Installer** (Setup.exe, NSIS din Tauri) cu helper-ul și PresentMon (amprenta verificată la build).
  **Dezinstalarea anulează întâi toate modificările**, apoi șterge aplicația. Testat în CI: instalare → aplicare
  „Recomandate” → dezinstalare → registrul identic cu înainte.
  *(2026-11-22: instalare în Program Files, pentru toți utilizatorii (decizia proprietarului). Dezinstalarea rulează
  `pco undo --all`; dacă ceva nu se poate anula, întreabă (o dezinstalare silențioasă se oprește). La update nu
  anulează nimic. Job-ul CI „installer” a trecut: 8 tweak-uri recomandate aplicate cu `pco` instalat, apoi
  dezinstalare, fiecare setare exact ca înainte. Mesajele proprii ale instalatorului Tauri apar în engleză și în
  varianta română (Tauri nu le are traduse).)*
- [ ] **3.16 Semnarea**: tu faci cererea la SignPath Foundation (îți dau pașii); când e aprobată, CI semnează
  automat. Până atunci, installer nesemnat, cu explicația avertismentului „Windows protected your PC” în README.
  *(Pregătit: pașii în `docs/SEMNARE.md`, politica de semnare și explicația avertismentului în README. Rămâne
  cererea proprietarului.)*
- [x] **3.17 Prima beta**: pre-release pe GitHub Releases, README cu capturi, instrucțiuni pentru beta-testeri.
  *(2026-11-22: publicat cu acordul proprietarului: <https://github.com/AldeaG72/pc-optimizer/releases/tag/v0.3.0-beta.1>,
  construit și testat de `release.yml` (instalare → aplicare → dezinstalare) înainte de publicare. Ghidul de testare:
  `docs/TESTARE-ETAPA-3.md`. Capturile din README rămân pentru după primul test pe un PC real.)*

**Închiderea etapei (2026-11-22):** proprietarul a decis să treacă peste testul din Windows Sandbox (pe laptop
Sandbox e oprit și virtualizarea pare oprită din BIOS). Criteriul 1 rămâne deci neîndeplinit: aplicația instalată nu
a fost încă folosită de un om pe un Windows 11 obișnuit; o fac beta-testerii. Criteriul 4 (nicio cerere pe internet)
e respectat în cod (CSP-ul ferestrei permite doar comunicarea internă, nicio bibliotecă de rețea), dar nu are încă
un test automat dedicat. Criteriile 2, 3, 5, 6: îndeplinite. Semnarea (3.16) așteaptă cererea proprietarului.

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
