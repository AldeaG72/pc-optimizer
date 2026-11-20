# Etapa 0 — Fundația: rețete, jurnal, undo, CLI

**Scop:** un program în linie de comandă (`pco`) care citește rețete YAML, arată ce ar schimba,
aplică modificările în siguranță și le anulează exact. Fără interfață grafică încă.
Asta e funcția pe care proprietarul a ales-o ca „de făcut perfect prima”.

## Sarcini

- [x] **0.1 Workspace Cargo**: `crates/core`, `crates/cli` (binar `pco`). `LICENSE` (textul complet GPL-3.0), `README.md` minimal, `.gitignore`, `rust-toolchain.toml` (stable).
- [x] **0.2 Schema rețetelor**: tipurile Rust (serde) pentru `recipes/schema/recipe.schema.json`. Validare la încărcare: orice rețetă invalidă oprește programul cu un mesaj clar (fișier + câmp).
- [x] **0.3 Backend-uri cu trait**: `RegistryBackend` (citește/scrie DWORD, QWORD, SZ; șterge valoare; deosebește „lipsă” de „0”) și `ServiceBackend` (citește/scrie tipul de pornire; „serviciul nu există” → rețeta e „nu se aplică pe acest PC”). Implementare reală cu crate-ul `windows` + implementare în memorie pentru teste.
- [x] **0.4 Jurnalul SQLite**: tabele `sessions` (id, start, status: pending/applied/rolled_back/failed) și `entries` (session_id, recipe_id, action_index, kind, target, previous_state JSON, new_state JSON, status). Migrări versionate.
- [x] **0.5 Motorul**: `plan` (stare curentă → stare țintă, per acțiune; detectează „deja aplicat” și „modificat în afara aplicației”) → `apply` (write-ahead: `pending` → scriere → verificare → `applied`; dacă o acțiune din rețetă eșuează, rețeta se anulează integral, iar celelalte rețete continuă; raport la final) → `undo` (restaurează exact starea salvată; șterge valoarea dacă nu exista).
- [x] **0.6 Recuperare după întrerupere**: la pornire, orice sesiune `pending` se anulează automat; mesaj pentru utilizator. *(Cu aceleași gărzi ca orice modificare: dacă rulează un joc sau e nevoie de admin, `pco` anunță și așteaptă; comenzile care doar citesc merg mai departe, cu un avertisment.)*
- [x] **0.7 Gărzi**: Windows 11 x64 build ≥ 22000 (altfel blocare), drepturi de admin necesare doar pentru `apply`/`undo` pe chei HKLM și servicii, blocare dacă rulează procese de joc/anti-cheat dintr-o listă configurabilă (`config/blocked-processes.yaml`: ex. `vgc.exe`, `FACEIT.exe`, `EasyAntiCheat.exe`, `BEService.exe`, `cs2.exe`, `VALORANT-Win64-Shipping.exe`, `FortniteClient-Win64-Shipping.exe`, `r5apex.exe`, `League of Legends.exe`, `dota2.exe`). Numele de proces trebuie verificate. *(Verificate, cu sursă, în `config/blocked-processes.yaml`. Clientul FACEIT e `faceitclient.exe`; `FACEIT.exe` nu apare în nicio sursă și rămâne marcat neverificat. Adăugate: `EasyAntiCheat_EOS.exe`, `BEService_x64.exe`, `r5apex_dx12.exe`, `FortniteClient-Win64-Shipping_EAC_EOS.exe`.)*
- [x] **0.8 CLI `pco`**:
  - `pco list` — toate rețetele, cu categorie, risc, stare (aplicat / neaplicat / modificat în afara aplicației / nu se aplică);
  - `pco plan <id>...` — diferențele „valoare veche → valoare nouă”, fără să scrie nimic;
  - `pco apply <id>... [--dry-run]`;
  - `pco undo <id>... | --all`;
  - `pco status` — sesiuni din jurnal.
  Mesajele pentru utilizator sunt în română sau engleză, după limba Windows, cu override `--lang ro|en`.
- [ ] **0.9 Rețetele inițiale** din `recipes/` (verifică fiecare cheie în documentația Microsoft și corectează unde e cazul; adaugă `sources`).
- [ ] **0.10 Teste**:
  - unitare pe backend-ul în memorie: plan, apply, undo, „lipsă vs. 0”, eșec la mijlocul unei rețete, recuperare după `pending`;
  - de integrare (doar pe Windows) pe cheia `HKCU\Software\PcOptimizerTest`;
  - pentru fiecare rețetă reală, în CI pe `windows-latest`: snapshot înainte → apply → verify → undo → snapshot identic.
- [ ] **0.11 CI GitHub Actions**: `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test` (Linux + Windows), testul de rețete pe `windows-latest`.
- [ ] **0.12 Instrucțiuni de test pentru proprietar**: `docs/TESTARE-ETAPA-0.md`, cu pașii exacți pentru mașina virtuală Windows 11 (comenzi de copiat).

## Criterii de ieșire

1. Toate rețetele inițiale trec în CI apply → undo fără nicio diferență în registru.
2. Testul „întrerupere la mijloc” lasă sistemul exact ca înainte după repornirea `pco`.
3. Proprietarul a rulat `pco list`, `pco plan`, `pco apply`, `pco undo` în mașina virtuală, urmând `docs/TESTARE-ETAPA-0.md`, fără erori.
4. `cargo clippy` și `cargo test` curate.

## În afara Etapei 0
Interfața Tauri, `elevated-helper`, benchmark, verificări de sănătate, setări de driver, mod joc.
