# Etapa 1 — Detectare hardware și verificări de sănătate (`pco health`)

**Scop:** `pco health` arată, într-o listă scurtă, hardware-ul detectat și problemele care strică
performanța în jocuri (SPEC 4.1), cu explicație și cu cum se rezolvă. **Nu schimbă nimic**: Etapa 1
doar citește (decizie 2026-11-21, vezi SPEC 4.1).

Asta deblochează și rețetele care depind de tipul PC-ului (desktop/laptop), blocate în Etapa 0.

## Sarcini

- [x] **1.1 Modelul datelor**: o „fotografie” a PC-ului (`HwInfo`): Windows, procesor, memorie, plăci video,
  monitoare, discuri, alimentare, securitate, programe instalate, jocuri. Se poate salva ca JSON
  (pentru teste și, mai târziu, pentru exportul de loguri anonimizat). Fără nume de utilizator/PC.
- [x] **1.2 Detectarea reală (Windows, doar citire)**, în spatele unui trait, ca în Etapa 0:
  - **Windows:** build, ediție, PC administrat de organizație (domeniu / Intune).
  - **Procesor:** nume, nuclee, fire.
  - **Memorie:** fiecare modul — capacitate, viteza nominală vs. viteza la care rulează, slot/canal.
  - **Plăci video:** nume, producător, integrată/dedicată, memorie video, versiunea și data driverului.
  - **Monitoare:** fiecare — rezoluția și frecvența actuale, rezoluția nativă și frecvența maximă, HDR pornit.
    Auto HDR și „Variable refresh rate” din Windows.
  - **Discuri:** SSD/HDD/NVMe, starea de sănătate (SMART, cum o raportează Windows), spațiu liber pe fiecare
    partiție, TRIM.
  - **Alimentare:** planul activ, pe baterie/priză, **desktop sau laptop** (tip carcasă + baterie).
  - **Securitate:** VBS și Memory Integrity (HVCI) active.
  - **Programe:** alte optimizatoare instalate (conflict), aplicații cu overlay (Discord, Steam, NVIDIA, Xbox).
  - **Jocuri:** cele 6 jocuri de la lansare, din Steam, Epic, Riot și EA app — unde sunt instalate și ce
    preferință de placă video au în Windows.
- [x] **1.3 Verificările** (funcții pure peste `HwInfo`, testabile pe orice sistem). Fiecare rezultat are:
  gravitate (problemă / atenție / info / OK), de ce contează, cum se rezolvă, rețeta potrivită dacă există.
  | Verificare | Problemă când |
  | --- | --- |
  | RAM sub viteza nominală | modulele rulează sub viteza lor (XMP/EXPO oprit) — doar explicăm, BIOS-ul nu se atinge |
  | RAM single-channel | un singur modul, sau toate pe același canal |
  | Frecvența monitorului | rulează sub maximul suportat la rezoluția actuală |
  | Rezoluție nativă | rezoluția actuală nu e cea nativă |
  | HDR / Auto HDR | pornite (pentru competitiv recomandăm oprit) — informativ |
  | Driver video | mai vechi de 6 luni → link oficial (NVIDIA / AMD / Intel) |
  | VBS / Memory Integrity | active — explicație (costul de FPS, riscul de securitate dacă se opresc) |
  | Plan de alimentare | „Economisire energie” pe desktop; laptop pe baterie |
  | Joc pe placa integrată | PC cu placă dedicată, dar jocul are preferința „economisire energie” |
  | Joc pe HDD | jocul e instalat pe un HDD |
  | TRIM | dezactivat |
  | SMART | Windows raportează discul ca nesănătos |
  | Spațiu liber | sub 10% sau sub 20 GB pe discul Windows sau pe discul unui joc |
  | Alte optimizatoare | instalate (pot da conflict) |
  | Overlay-uri | listate, cu cum se opresc — informativ |
  | PC administrat | domeniu / Intune — unele setări pot fi suprascrise |
- [x] **1.4 Comanda `pco health`** (RO/EN): întâi hardware-ul pe scurt, apoi rezultatele, cele mai grave primele.
  `--json` pentru export. Funcționează fără drepturi de administrator (ce cere admin apare „necunoscut”).
- [x] **1.5 Desktop/laptop în gărzi**: tipul detectat ajunge la rețete (`platform.form_factor`).
- [x] **1.6 Teste**:
  - verificările: pe „fotografii” de test (laptopul proprietarului — grafică Intel integrată; desktop gaming
    NVIDIA; laptop cu două plăci, pe baterie; runner-ul CI), pentru fiecare caz problemă / OK / necunoscut;
  - citirea bibliotecilor de jocuri (Steam, Epic, Riot, EA) pe fișiere de exemplu;
  - detectarea reală pe Windows: rulează pe PC-ul de dezvoltare și în CI (doar citește) și dă valori plauzibile.
- [x] **1.7 CI**: `pco health` și `pco health --json` rulează pe Windows.
- [ ] **1.8 Testul proprietarului**: rulezi `pco health` pe laptopul tău (doar citește — permis) și îmi spui dacă
  hardware-ul și problemele arătate corespund realității. ~5 minute.

## Criterii de ieșire

1. `pco health` rulează fără erori pe laptopul proprietarului și în CI; hardware-ul detectat corespunde realității.
2. Fiecare verificare are teste pentru „problemă”, „OK” și „necunoscut”.
3. Nimic nu se scrie: detectarea nu are deloc funcții de scriere.
4. `cargo clippy` și `cargo test` curate, CI verde.

## În afara Etapei 1

Repararea automată (vine cu interfața), verificarea online a driverelor, MUX switch și starea reală
G-Sync/FreeSync (cer biblioteci NVIDIA/AMD — Etapa 4), temperaturi (decizie deschisă: LibreHardwareMonitor),
benchmark (Etapa 2).
