# Start aici — cum pornești proiectul cu Claude Code

Durează cam o oră, o singură dată. Toate comenzile se rulează în **PowerShell**
(Start → scrie „PowerShell” → Enter). Unde scrie „ca administrator”: click dreapta → „Run as administrator”.

## 1. Instalează uneltele (pe laptopul tău)

Copiază comenzile pe rând. După fiecare, închide și redeschide PowerShell.

```powershell
winget install --id Git.Git -e
winget install --id Rustlang.Rustup -e
winget install --id OpenJS.NodeJS.LTS -e
winget install --id Microsoft.VisualStudio.2022.BuildTools -e --override "--quiet --wait --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"
```

Apoi instalează Claude Code (comanda oficială pentru Windows):

```powershell
irm https://claude.ai/install.ps1 | iex
```

Verifică: `git --version`, `cargo --version`, `node --version`, `claude --version`. Fiecare trebuie să afișeze un număr de versiune.

> Dacă `winget` nu merge: actualizează „App Installer” din Microsoft Store.

## 2. Creează repo-ul pe GitHub

1. Intră pe github.com → **New repository**.
2. Nume provizoriu (ex.: `pc-optimizer`), **Public**, bifează „Add a README”, licență **GNU GPLv3**.
3. Pe pagina repo-ului: buton verde **Code** → copiază adresa HTTPS.
4. În PowerShell:

```powershell
cd $HOME\Documents
git clone <adresa-copiată>
cd pc-optimizer
```

## 3. Pune fișierele din acest pachet în repo

Copiază **tot conținutul** acestui folder (`CLAUDE.md`, `docs/`, `recipes/`, `PROMPT-START.md`, `START-AICI.md`)
în folderul `pc-optimizer` de la pasul 2. Dacă te întreabă de `README.md`, păstrează-l pe cel existent.

## 4. Pornește Claude Code

```powershell
cd $HOME\Documents\pc-optimizer
claude
```

La prima pornire te loghezi cu contul Claude. Apoi deschide `PROMPT-START.md`, copiază
textul de sub linie și lipește-l ca prim mesaj.

Claude Code o să-ți ceară permisiunea înainte să ruleze comenzi sau să modifice fișiere.
Citește ce cere; pentru comenzile `cargo` și `git` din proiect poți aproba liniștit.

## 5. Mașina virtuală de test (înainte de finalul Etapei 0)

Tweak-urile reale **nu** se testează pe laptopul tău de programare, ci într-o mașină virtuală Windows 11.

- **Windows 11 Pro:** activează Hyper-V (PowerShell ca administrator):
  `Enable-WindowsOptionalFeature -Online -FeatureName Microsoft-Hyper-V -All`, repornește,
  apoi deschide **Hyper-V Manager** → **Quick Create** → **Local installation source** și alege imaginea ISO de Windows 11.
- **Windows 11 Home:** instalează VirtualBox (`winget install --id Oracle.VirtualBox -e`) și folosește aceeași imagine ISO.
- Imaginea ISO o descarci gratuit de pe pagina oficială Microsoft „Download Windows 11” (secțiunea „Download Windows 11 Disk Image (ISO)”). Pentru teste, Windows poate rula și neactivat.

Înainte de fiecare test, fă un **checkpoint** (Hyper-V) sau un **snapshot** (VirtualBox) al mașinii virtuale.
Dacă ceva se strică, revii la el dintr-un clic. Claude Code îți va scrie pașii exacți în `docs/TESTARE-ETAPA-0.md`.

## 6. Ritmul de lucru

- O sesiune = un pas din `docs/ETAPA-0.md`.
- La finalul sesiunii, Claude Code îți spune ce să testezi. Testezi doar ce-ți cere, în mașina virtuală.
- Dacă te întreabă ceva de produs care nu e în `docs/SPEC.md`, răspunsul tău devine regulă: cere-i să-l adauge în `docs/SPEC.md`.
