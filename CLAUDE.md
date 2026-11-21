# CLAUDE.md — PC Optimizer (nume provizoriu; numele final se alege mai târziu)

Acest fișier e citit automat de Claude Code la fiecare sesiune. Conține regulile proiectului.
Specificația completă: `docs/SPEC.md`. Etapa curentă: `docs/ETAPA-3.md` (gata: `docs/ETAPA-0.md`, `docs/ETAPA-1.md`, `docs/ETAPA-2.md`). Ce refuzăm și de ce: `docs/CE-NU-FACEM.md`.

## Despre proiect

Optimizator open-source (GPL-3.0) pentru Windows 11, pentru gameri competitivi
(CS2, Valorant, Fortnite, Apex, League of Legends, Dota 2). Modifică doar setări
documentate din Windows și din driverul video, fiecare explicată și reversibilă,
și măsoară efectul cu benchmark (PresentMon). Fără BIOS, fără overclocking,
fără atingerea proceselor jocurilor.

Stack: Rust (workspace Cargo) + Tauri 2 pentru interfață (din Etapa 3). Interfață în română + engleză.

## Cine lucrează aici

- **Proprietarul proiectului nu e programator.** Explică-i în română, pe scurt și fără jargon,
  ce ai făcut și ce trebuie să testeze. Când îi ceri să ruleze ceva, dă-i comanda exactă,
  gata de copiat, și spune-i în ce fereastră s-o ruleze.
- Tu scrii codul. El testează și decide. Când o decizie de produs nu e acoperită
  de `docs/SPEC.md`, întreabă-l înainte să construiești. Nu ghici.
- Testarea lui e limitată: sub 2 ore pe săptămână, pe un laptop de programare cu
  **grafică integrată Intel, fără placă video dedicată**. Testele automate trebuie să prindă
  aproape totul. Ce ține de NVIDIA/AMD se testează doar la beta-testeri.

## Reguli de siguranță (nenegociabile)

1. **Nu aplica niciodată tweak-uri reale pe PC-ul de dezvoltare.** Testele care modifică
   registrul folosesc doar o cheie de test (`HKCU\Software\PcOptimizerTest\...`) sau un
   backend simulat. Testarea pe setări reale se face doar într-o mașină virtuală Windows 11
   sau pe runner-ele `windows-latest` din GitHub Actions.
2. **Fiecare modificare are undo.** Înainte de orice scriere, starea originală exactă
   (inclusiv „cheia/valoarea nu exista”) se salvează în jurnal. Fără jurnal, fără scriere.
3. **Write-ahead:** în jurnal se scrie întâi `pending`, apoi se face modificarea, apoi `applied`.
   La pornire, orice sesiune rămasă `pending` (pană, crash) se anulează automat.
4. Nu dezactiva niciodată Windows Defender, firewall-ul, update-urile de securitate,
   UAC, SmartScreen. Nu modifica fișierul `hosts`. Nu adăuga reguli de firewall.
5. Nu injecta cod în alte procese, nu instala drivere kernel, nu modifica executabile
   de jocuri, nu schimba prioritatea proceselor jocurilor.
6. Aplicarea e **blocată** dacă rulează un joc sau un anti-cheat cunoscut (Vanguard, FACEIT,
   Easy Anti-Cheat, BattlEye etc.): mesaj „Închide jocul întâi”.
7. Aplicația rulează **doar** pe Windows 11 x64 (build ≥ 22000). Pe Windows 10 sau ARM:
   blocare cu mesaj clar.
8. **Fără telemetrie.** Singurele cereri de rețea permise: ping/test de rețea către serverele
   jocurilor și verificarea versiunii de driver video. Nimic altceva fără aprobarea proprietarului.
9. Un tweak nou intră doar ca rețetă YAML validată de schemă, cu `why`, `breaks`, `risk`
   și `sources`. Niciun tweak „magic” fără sursă (vezi `docs/CE-NU-FACEM.md`).

## Arhitectură (pe scurt)

```
crates/core      logica, fără UI: recipe/, engine/, journal/, actions/, health/, bench/, hw/
crates/cli       `pco` — plan | apply | undo | status | list
crates/elevated-helper   (Etapa 3) proces cu drepturi de admin, acceptă doar ID-uri de rețete
                         (și benchmark-ul: nume de joc/.exe doar pentru PresentMon, fără schimbări;
                         programele de la pornire: ID-uri din propria lui listă)
app/             (Etapa 3) Tauri 2 + UI
recipes/         rețetele YAML + schema JSON
```

- Acțiunile Windows sunt în spatele unui trait (ex.: `RegistryBackend`, `ServiceBackend`),
  cu implementare reală (crate-ul `windows`) și una în memorie pentru teste.
  Așa, majoritatea testelor rulează pe orice sistem, inclusiv Linux.
- Jurnalul de undo e SQLite (`rusqlite`), în `%LOCALAPPDATA%\<app>\journal.db`.

## Convenții de cod

- Rust stable, ediția 2021+, `cargo fmt` și `cargo clippy -- -D warnings` curate înainte de commit.
- Erori cu `thiserror` în biblioteci, `anyhow` doar în CLI. Fără `unwrap()` în cod de producție.
- Loguri cu `tracing`, local, fără date personale (fără nume de utilizator/PC/IP în export).
- Cod și comentarii în engleză; textele pentru utilizator în fișiere de traducere RO/EN.
- Commit-uri mici, mesaje clare (stil Conventional Commits: `feat:`, `fix:`, `test:`, `docs:`).
- Fiecare funcționalitate nouă vine cu teste. Fiecare rețetă trece testul automat
  apply → verify → undo → „registrul e identic cu înainte”.

## Cum lucrezi într-o sesiune

1. Citește `docs/ETAPA-0.md` (sau etapa curentă) și bifează ce e gata.
2. Propune un plan scurt, apoi implementează pas cu pas, cu teste.
3. La final: rulează `cargo fmt`, `cargo clippy`, `cargo test`; spune-i proprietarului,
   în română, ce s-a schimbat și ce poate testa (cu comenzi exacte).
4. Nu face push pe `main` fără ca testele să treacă.
