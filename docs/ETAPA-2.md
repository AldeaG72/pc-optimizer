# Etapa 2 — Benchmark cu PresentMon (`pco bench`)

**Scop:** măsori FPS-ul într-un joc înainte și după tweak-uri, iar aplicația îți spune dacă a ajutat:
FPS mediu, 1% low, frametime mediu, „înainte → după (+X%)”. Dacă FPS-ul scade cu adevărat, îți propune
anularea (SPEC 4.4; deciziile din 2026-11-21 sunt în SPEC).

## Cum arată pentru utilizator (linie de comandă)

1. `pco bench cs2 --before` → pornește jocul (sau îl folosește dacă e deja pornit).
2. Intri în meci și apeși **Enter**. Urmează 10 s de așteptare, apoi 60 s de măsurare. Se repetă de 3 ori.
3. `pco apply …` — aplici tweak-urile.
4. `pco bench cs2 --after` → la fel, încă 3 rulări.
5. La final: `FPS mediu 142 → 151 (+6,3%)`, `1% low 88 → 97 (+10,2%)`, `frametime 7,0 → 6,6 ms`.
   Dacă FPS-ul a scăzut cu adevărat: mesaj și comanda exactă de anulare.

Jocurile fără o hartă fixă de test (VALORANT, League of Legends) apar cu „mai puțin precis”.

## Sarcini

- [ ] **2.1 PresentMon inclus**: versiunea oficială fixată (GitHub GameTechDev/PresentMon), amprentă SHA-256 verificată,
  descărcată de un script (`scripts/fetch-presentmon.ps1`) — nu se pune în git. Licența MIT în `THIRD-PARTY.md`.
  Îți cer acordul înainte de prima descărcare.
- [ ] **2.2 Rularea PresentMon**: pornit pentru procesul jocului, 60 s, rezultatul într-un fișier CSV temporar.
  Cere drepturi de administrator (Windows le cere pentru măsurare). Fără overlay în joc (SPEC).
- [ ] **2.3 Calculul**: din CSV — FPS mediu (cadre / timp), 1% low (FPS la frametime-ul percentilei 99),
  frametime mediu. Cadrele pierdute și rândurile stricate se tratează explicit.
- [ ] **2.4 Istoricul**: tabel nou în jurnalul SQLite (migrarea 2): joc, „înainte/după”, data, rezultatele fiecărei
  rulări. `pco bench history` și `pco bench forget` (ștergere).
- [ ] **2.5 Comparația și verdictul**: media celor 3 rulări „înainte” vs. „după”; scădere reală = peste 3% **și**
  peste variația dintre rulări → propunerea de anulare, cu comanda exactă (`pco undo …`).
- [ ] **2.6 Comanda `pco bench`** (RO/EN): jocurile detectate în Etapa 1 (CS2, Dota 2, Apex, Fortnite, VALORANT, LoL)
  sau orice proces (`--process nume.exe`); pornirea jocului prin launcher (Steam, Epic, Riot, EA); Enter → 10 s → 60 s;
  oprire curată cu Ctrl+C. Blocat dacă PresentMon lipsește sau amprenta nu se potrivește.
- [ ] **2.7 Teste**:
  - calculul, pe fișiere CSV de exemplu (stabil, cu sacade, cu cadre pierdute, gol, stricat);
  - verdictul (creștere, scădere reală, scădere în zgomot);
  - istoricul și migrarea jurnalului;
  - **în CI, pe Windows**: o aplicație mică de test care desenează cadre, măsurată de PresentMon cap-coadă.
- [ ] **2.8 Testul proprietarului**: pe laptopul tău nu e niciun joc dintre cele 6, așa că măsori o pagină web cu
  animație 3D (`pco bench --process msedge.exe`) — câteva minute. Măsurarea doar citește, deci e permisă.

## Criterii de ieșire

1. În CI, PresentMon măsoară aplicația de test, iar rezultatele sunt plauzibile (FPS > 0, 1% low ≤ FPS mediu).
2. Calculul și verdictul au teste pentru fiecare caz de mai sus.
3. PresentMon rulează doar cu amprenta verificată; altfel `pco bench` refuză.
4. Proprietarul a rulat `pco bench` pe laptop și rezultatul i se pare rezonabil.
5. `cargo clippy` și `cargo test` curate, CI verde.

## În afara Etapei 2

Interfața grafică și butoanele „Înainte/După” din fluxul Tweak-uri (Etapa 3), alegerea planului de alimentare pe
baza benchmark-ului (după ce avem rețetele de alimentare), overlay în joc (nu se face niciodată — SPEC).
