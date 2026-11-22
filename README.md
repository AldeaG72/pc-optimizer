# PC Optimizer (nume provizoriu / working title)

**RO** — Optimizator open-source pentru Windows 11, pentru gameri competitivi.
Modifică doar setări documentate din Windows, fiecare explicată, cu sursă și cu undo exact.
Fără BIOS, fără overclocking, fără atingerea jocurilor, fără telemetrie.

**EN** — Open-source optimizer for Windows 11, for competitive gamers.
It only changes documented Windows settings, each one explained, sourced and exactly reversible.
No BIOS, no overclocking, no touching games, no telemetry.

> ⚠️ **Stadiu / Status:** Etapa 3 — prima beta, pentru testeri.
> Stage 3 — first beta, for testers.

## Instalare (beta) / Installing (beta)

**RO** — Descarcă `PC Optimizer_<versiune>_x64-setup.exe` din [Releases](https://github.com/AldeaG72/pc-optimizer/releases)
(versiunile „Pre-release” sunt beta). Cere Windows 11 x64. Dezinstalarea anulează întâi toate modificările făcute
de aplicație.

Beta-ul **nu e încă semnat digital** (cererea de semnare gratuită la SignPath Foundation e în pregătire). De aceea Windows poate
afișa „Windows protected your PC”: apasă **More info › Run anyway** doar dacă ai descărcat fișierul de pe pagina
de Releases de mai sus. Antivirusul poate cere și el confirmare.

**EN** — Download `PC Optimizer_<version>_x64-setup.exe` from [Releases](https://github.com/AldeaG72/pc-optimizer/releases)
("Pre-release" versions are betas). Needs Windows 11 x64. Uninstalling first undoes every change the app made.

The beta is **not code-signed yet** (a free signing request to SignPath Foundation is being prepared), so Windows may show
"Windows protected your PC": click **More info › Run anyway** only if you downloaded the file from the Releases page
above. Your antivirus may ask too.

## Politica de semnare / Code signing policy

**RO** — Fișierele publicate sunt construite numai de GitHub Actions din codul public al acestui repository
(job-ul `installer` din `.github/workflows/ci.yml`). Doar proprietarul proiectului
([@AldeaG72](https://github.com/AldeaG72)) aprobă și publică versiunile. Aplicația nu trimite date nicăieri
(fără telemetrie). Când semnarea va fi aprobată: *Free code signing provided by SignPath.io, certificate by
SignPath Foundation.*

**EN** — Published files are built only by GitHub Actions from this repository's public code (the `installer` job
in `.github/workflows/ci.yml`). Only the project owner ([@AldeaG72](https://github.com/AldeaG72)) approves and
publishes releases. The app sends no data anywhere (no telemetry). Once signing is approved: *Free code signing
provided by SignPath.io, certificate by SignPath Foundation.*

## Ce nu facem / What we don't do

Vezi [`docs/CE-NU-FACEM.md`](docs/CE-NU-FACEM.md).

## Licență / License

[GPL-3.0](LICENSE)
