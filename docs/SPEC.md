# Specificație — PC Optimizer (nume provizoriu)

Toate deciziile de produs luate cu proprietarul proiectului. Dacă ceva nu e aici, întreabă.

## 1. Produs

| Subiect | Decizie |
| --- | --- |
| Public | Gameri competitivi |
| Prioritate | Performanța în jocuri (FPS mediu, 1% low, input lag, echilibrat); folosirea zilnică e secundară |
| Jocuri la lansare | CS2, Valorant, Fortnite, Apex, League of Legends, Dota 2 |
| Platformă | Doar Windows 11 x64. Windows 10 și ARM: blocat, cu mesaj clar |
| PC-uri administrate (domeniu, Intune) | Detectate, cu avertisment că unele setări pot fi suprascrise |
| Licență | GPL-3.0 |
| Bani | Gratuit. Donații: GitHub Sponsors + Patreon (beneficii: vot pe funcții, nume la „Mulțumiri”) |
| Limbi | Română + engleză; implicit după limba Windows |
| Cod | Repo GitHub public |
| Instalare | Installer clasic (Setup.exe). Update-uri manuale de pe GitHub Releases, fără anunț în aplicație |
| Dezinstalare | Anulează automat toate modificările |
| Telemetrie | Niciuna |
| BIOS | Nu se atinge. Doar detectare + explicații (ex.: XMP/EXPO oprit) |
| Overlay în joc | Nu |

## 2. Principii

1. Transparent: fiecare tweak e o rețetă YAML publică, cu explicație și sursă.
2. Reversibil: undo pe fiecare tweak, din starea reală salvată înainte. Punctul de restaurare Windows e opțional.
3. Măsurabil: benchmark înainte/după; dacă FPS-ul scade, aplicația propune anularea.
4. Sigur implicit: tweak-urile avansate sunt ascunse până la activare explicită.
5. Nu slăbește securitatea și nu atinge jocurile.

## 3. Ecrane și interfață

Mockup: canvasul „Mockup – PC Optimizer” (5 ecrane: Acasă, Tweak-uri, Setări, Tur, Aplicare).

| Element | Decizie |
| --- | --- |
| Navigație | Bară laterală stânga, pliabilă: Acasă, Tweak-uri, Setări |
| Stil | Minimalist (Fluent/Windows 11), fundal Mica, umbre ușoare, colțuri 8–12 px, fără animații |
| Culori | Accent albastru safir #1D4ED8 (și comutatorul „aplicat”). Risc: verde Sigur / galben Moderat / roșu Avansat. Câștig verde, scădere roșu |
| Temă | Urmează Windows (light/dark), alegere manuală în Setări |
| Font / iconițe | Inter + Lucide |
| Fereastră | ~1100×720, redimensionabilă, bară de titlu personalizată |
| Ton | Prietenos și direct, detalii tehnice la click |
| Prima pornire | Tur de 4 ecrane cu iconiță mare + text: limbă; ce face / ce nu face; totul se poate anula; recomandare benchmark + declinarea răspunderii cu „Sunt de acord” (și în Despre) |
| Acasă | Hardware detectat (listă compactă), probleme ca și carduri cu „Rezolvă”, rezumat tweak-uri, starea rețelei, card Mod joc (comutator + Configurează). Fără scor de performanță. Fără rezultate de benchmark |
| Tweak-uri | Pe categorii, listă aerisită, bară de căutare. Fiecare tweak: nivel de risc, impact estimat, ce poate strica, detalii tehnice într-un panou lateral dreapta. Comutator albastru când e aplicat. Etichetă „modificat în afara aplicației” când starea reală diferă de jurnal |
| Recomandate | Buton în fiecare categorie + unul global; profil Laptop separat. Doar tweak-uri Sigur potrivite PC-ului |
| Aplicare | Bifele se adună → „Aplică tot” (cere UAC doar acum) → listă live cu bifă/eroare per tweak; erorile se sar și se raportează la final → restart propus imediat dacă e nevoie |
| Setări | Limbă, temă, pornire cu Windows/tray, „Afișează tweak-urile avansate” (avertisment + confirmare la activare), punct de restaurare opțional, backup zilnic registru, istoric benchmark (opțional), Depanare, „Anulează tot”, „Exportă loguri (anonimizate)”, Despre |
| Ecrane goale | Iconiță + mesaj + buton |
| Notificări | Notificare Windows discretă la pornirea/oprirea modului joc |
| Scurtături de tastatură | Nu |
| Iconiță tray | Logo color |

## 4. Funcționalități

### 4.1 Verificări de sănătate (rulează primele)
RAM sub viteza nominală (XMP/EXPO, prin WMI) · RAM single-channel · refresh rate sub maxim (DXGI) ·
G-Sync/FreeSync, HDR/Auto HDR pornite în competitiv, scalare/rezoluție nativă · driver video
mai nou disponibil (doar avertisment + link oficial, fără descărcare) · VBS/HVCI activ (explicație) ·
plan de alimentare · laptop pe baterie · jocul rulează pe iGPU în loc de placa dedicată · MUX switch ·
jocul instalat pe HDD · TRIM · SMART · spațiu liber · alte optimizatoare instalate (conflict) ·
overlay-uri (Discord, Steam, NVIDIA, Xbox: listate + cum se opresc) · PC administrat de organizație.
Pe mai multe monitoare: întreabă pe ce monitor joci.

Decizii pentru Etapa 1 (2026-11-21):
- **Driver video:** doar local — versiunea și data driverului instalat; avertisment dacă e vechi, cu linkul oficial al producătorului. Fără verificare online deocamdată.
- **Jocuri:** detectarea jocurilor instalate (Steam, Epic, Riot, EA app) se face din Etapa 1, ca să meargă verificările „jocul e pe HDD” și „jocul rulează pe placa integrată”. Etapa 2 o refolosește.
- **Mai multe monitoare:** în linia de comandă se verifică toate monitoarele, fiecare afișat separat; întrebarea „pe care joci?” vine în interfață (Etapa 3).
- **`pco health` doar explică:** arată problema, de ce contează și cum se rezolvă (cu rețeta potrivită, unde există). Nu schimbă nimic. Butonul „Rezolvă” vine în interfață (Etapa 3).

### 4.2 Tweak-uri (catalog inițial; fiecare devine rețetă YAML)
- **Alimentare:** plan ales după benchmark pe desktop; profil Laptop separat; Power Throttling + Core Parking oprite (Moderat, doar desktop); USB selective suspend oprit (desktop; laptop doar pe încărcător).
- **Gaming:** Game Mode pornit; Game Bar oprit opțional; Game DVR oprit; optimizări pentru jocuri în fereastră; HAGS recomandat după benchmark; MPO oprit (Moderat, doar la flicker/sacade).
- **Input:** accelerația mouse-ului oprită; scurtăturile Sticky/Filter Keys oprite; test polling rate.
- **Windows:** „End Task” la click dreapta pe taskbar; amânarea update-urilor de funcții (doar Pro, securitatea neatinsă); manager complet de aplicații la pornire (impact, editor, ce face, recomandare); notificări în timpul jocului oprite.
- **Rețea:** economisirea energiei pe placa de rețea și Energy Efficient Ethernet oprite (doar setări sigure).
- **Sunet:** recomandare Spatial sound / HRTF pe joc.
- **Debloat:** Copilot, Widgets, Recall (prin politici oficiale); OneDrive; aplicații preinstalate (reinstalabile prin comutator); Edge doar oprit în fundal, nu dezinstalat.
- **Confidențialitate** (categorie separată, inclusă în Recomandate global): telemetrie la minim, ID de publicitate oprit, istoric de activitate oprit, sugestii și căutare web din Start oprite.
- **Curățare:** fișiere temporare, resturi Windows Update, cache-uri de launchere, coșul de gunoi (cu confirmare).
- **Avansat** (ascuns implicit): Memory Integrity/VBS oprit (avertisment de securitate), MSI mode pentru placa video, listă de servicii (toate afișate, modificabile doar cele dintr-o listă sigură).

### 4.3 Driver video (automat, cu undo)
- **NVIDIA:** Low Latency Mode On (în jocurile cu Reflex, se recomandă Reflex din joc, cu prioritate) · Power management: Prefer maximum performance · Shader cache: Unlimited · Texture filtering: High performance.
- **AMD:** Chill, Boost, AFMF oprite · Wait for Vertical Refresh: Always Off · Enhanced Sync oprit cu FreeSync · shader cache AMD Optimized · Image Sharpening oprit. **Anti-Lag nu se pornește din driver**; se recomandă Anti-Lag 2 din joc.
- **Intel:** nu la lansare (decizie deschisă: Intel Graphics Control Library).
- Plafon de FPS: nu se recomandă.

### 4.4 Benchmark
PresentMon (MIT). Utilizatorul alege jocul din listă (detectare automată Steam/Epic/Riot/Battle.net + adăugare manuală); aplicația îl pornește; după măsurare jocul rămâne deschis.
Măsurătoarea pornește automat la 10 s după intrarea în joc, durează 60 s; 3 rulări înainte + 3 după.
Afișează: FPS mediu, 1% low, frametime mediu, „înainte → după (+X%)”. Jocurile fără hartă fixă (Valorant, LoL) sunt măsurate în timpul jocului și marcate „mai puțin precis”. Dacă FPS-ul scade: „Anulez modificările?”.
Istoric opțional (listă simplă). Locul benchmark-ului: fluxul Tweak-uri (înainte/după „Aplică tot”).

### 4.5 Mod joc (în tray)
Detectează jocul pornit; fiecare acțiune se bifează separat: închide aplicațiile alese (listă sugerată + bife; se redeschid după), plan de alimentare maxim, notificări oprite, update-uri din fundal pe pauză. La ieșirea din joc, totul revine. Consum țintă: sub 30 MB RAM, CPU aproape zero. Nu atinge procesul jocului.

### 4.6 Recomandări din joc
Tabel per joc (setare → valoare → de ce), pe 3 niveluri de PC detectate automat, bazate pe setările pro-playerilor din surse publice. Fișierele de config ale jocurilor **nu** se modifică. CS2: doar opțiuni de lansare cu efect dovedit + lista celor inutile. Fortnite: „Modul Performanță” recomandat tuturor.

### 4.7 Rețea și mouse
Ping către serverele jocurilor (Valve, Riot, Epic, EU) + test de bufferbloat. Test polling rate.

### 4.8 Depanare (în Setări)
Resetare rețea (`netsh int ip reset` + `netsh winsock reset`), reparare Windows Update, scanare sistem (sfc + DISM, „ultimă soluție”), curățare cache shadere la cerere (arată dimensiunea, avertizează de sacade temporare), scurtături spre panourile Sunet, Opțiuni de alimentare, Conexiuni de rețea.

### 4.9 Siguranță
Undo per tweak din jurnal · write-ahead + anulare automată la pornire după o întrerupere · aplicare blocată cât rulează un joc cu anti-cheat · detectare update Windows care a resetat tweak-uri → întreabă dacă le reaplică · backup zilnic al registrului (opțional) · export loguri anonimizat (hardware, Windows, tweak-uri + erori, benchmark) · mesaj simplu dacă antivirusul blochează.

## 5. Comunitate și lansare
Discord (#suport cu template, #rezultate, #idei-tweak-uri), Reddit, GitHub Discussions, TikTok/YouTube.
Tweak-uri noi: vot pe Discord/Patreon → test cu benchmark → review → build. Utilizatorii nu pot importa rețete proprii.
README: capturi + GIF, ce face / ce nu face, FAQ anti-cheat și siguranță. Fără site separat.
Beta: grup selectat cu PC-uri diverse. Lansare v0.1: când e sigură, fără termen. Succes la 6 luni: număr de utilizatori.

## 6. Roadmap
- **Etapa 0** – CLI + motorul de rețete + jurnal + undo (vezi `docs/ETAPA-0.md`).
- **Etapa 1** – detectare hardware + verificări de sănătate (`pco health`).
- **Etapa 2** – benchmark cu PresentMon (`pco bench`).
- **Etapa 3** – aplicația Tauri, `elevated-helper`, profiluri, installer semnat (SignPath Foundation sau Azure Trusted Signing) → prima versiune beta, la grupul selectat.
- **Etapa 4** – mod joc, setări automate de driver NVIDIA/AMD, teste de rețea și mouse, recomandări pe joc. Fac parte din MVP: v0.1 public apare după această etapă.
- **După v0.1** – export/import configurație, suport Intel, istoric extins.

## 7. Decizii deschise
Framework UI (Svelte sau React) · numele + logo · suport driver Intel · LibreHardwareMonitor pentru temperaturi (cere driver) · cine recrutează beta-testerii.
