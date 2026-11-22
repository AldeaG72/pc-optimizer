# Etapa 4 — Modul joc, driverul video, rețea și mouse, recomandări pe joc → v0.1 public

**Scop:** ultimele funcții din MVP (SPEC 6), apoi prima versiune publică v0.1. Ca în Etapa 3, patru părți, fiecare cu
CI verde și unită separat în `main`, cu acordul proprietarului. Ce ține de NVIDIA/AMD se testează doar la
beta-testeri (laptopul proprietarului are grafică Intel).

## Partea A — Modul joc și iconița din tray (SPEC 4.5)

- [x] **4.1 Aplicația în fundal**: iconiță în tray (zona de lângă ceas), închiderea ferestrei o lasă în tray,
  „Pornește cu Windows” opțional din Setări (oprit implicit). Consum țintă în fundal: sub 30 MB RAM, CPU aproape zero.
- [x] **4.2 Detectarea jocului**: jocurile suportate și cele adăugate de mână, după numele procesului; fără să
  atingă procesul jocului.
- [x] **4.3 Acțiunile modului joc**, fiecare bifată separat, toate anulate la ieșirea din joc (și după o pană, prin
  jurnal): închide aplicațiile alese și le redeschide după; planul de alimentare maxim; notificările oprite;
  update-urile din fundal pe pauză.
- [x] **4.4 Cardul „Mod joc” pe Acasă** și notificarea scurtă la intrarea/ieșirea din joc.
  *(Cardul arată starea în timp real. Notificarea Windows la intrare/ieșire: nu încă.)*

## Partea B — Setările automate de driver video (SPEC 4.3)

- [ ] **4.5 NVIDIA** (prin interfața oficială NVIDIA pentru profilurile driverului): Low Latency Mode, Power
  management, Shader cache, Texture filtering — cu starea originală în jurnal și undo exact.
- [ ] **4.6 AMD** (prin interfața oficială AMD, ADLX): Chill, Boost, AFMF, VSync, Enhanced Sync, shader cache, Image
  Sharpening. Anti-Lag nu se pornește din driver.
- [ ] **4.7 Teste**: backend simulat pentru amândouă în CI; pe plăci reale doar la beta-testeri.

## Partea C — Rețea și mouse (SPEC 4.7)

- [ ] **4.8 Ping** către serverele jocurilor (Valve, Riot, Epic, EU) — singurele cereri de rețea permise
  (CLAUDE.md, regula 8).
- ~~**4.9 Test de bufferbloat**~~ — scos din v0.1 (decizia proprietarului, 2026-11-22): ar cere o descărcare de pe
  un server care nu e al unui joc. Revine după o decizie despre server.
- [ ] **4.10 Test de polling rate** pentru mouse (miști mouse-ul într-o fereastră; aplicația numără rapoartele).

## Partea D — Recomandări pe joc și v0.1 (SPEC 4.6)

- [ ] **4.11 Recomandările din joc**: pentru fiecare joc suportat, un tabel setare → valoare → de ce, pe 3 niveluri
  de PC detectate automat, din surse publice; fișierele jocurilor nu se modifică. CS2: doar opțiuni de lansare cu
  efect dovedit + lista celor inutile. Fortnite: „Modul Performanță”.
- [ ] **4.12 Restanțe din Etapa 3**: test automat „aplicația nu face cereri pe internet”, capturi în README,
  semnarea (după aprobarea SignPath).
- [ ] **4.13 v0.1 public**: release (nu pre-release), note de lansare, pagina de proiect.

## Teste

Ca până acum: backend simulat pentru tot ce schimbă ceva, CI pe Windows real pentru ce se poate (modul joc cu un
„joc” de test — programul `frame-demo`), beta-testeri pentru NVIDIA/AMD. Proprietarul: un test scurt pe laptop
doar pentru ce citește (ping, polling rate), fără aplicare.

## Criterii de ieșire

1. Modul joc pornește și oprește totul singur, iar după o pană nimic nu rămâne schimbat (test în CI).
2. Setările de driver au undo exact (test pe backend simulat; confirmat la cel puțin un beta-tester NVIDIA și unul AMD).
3. Singurele cereri de rețea sunt ping/testele către serverele jocurilor (verificat automat).
4. v0.1 publicat.

## Note Partea A (2026-11-22)

- Modul joc schimbă doar ce poate face aplicația fără drepturi de administrator: planul de alimentare activ și
  închiderea (politicoasă) a aplicațiilor alese, redeschise după joc. Starea originală se salvează într-un fișier
  **înainte** de orice schimbare; la pornire, dacă fișierul a rămas (pană, crash), totul revine.
- **Notificările (4.3):** setarea din registru are efect abia după un restart, deci nu merge cât joci. Decizia
  proprietarului: modul joc explică „Nu deranja” automat al Windows 11 „când joci un joc” și deschide setarea.
- **Descărcările (4.3):** decizia proprietarului: un tweak permanent, `network.background-downloads-limited`
  (Delivery Optimization, 20% pentru descărcările din fundal), aplicat o dată cu UAC și anulabil, în loc de o limită
  doar cât joci (ar cere drepturi de administrator în timpul jocului).
- Testat pe Windows real în CI: planul și Notepad închis/redeschis. „Pornește cu Windows” scrie doar intrarea
  aplicației în Run (utilizatorul curent); dezinstalarea o șterge.
