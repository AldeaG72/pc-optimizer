# Testarea Etapei 0 în mașina virtuală

Durează cam **o oră**. Testezi `pco` pe un Windows 11 adevărat, dar „de unică folosință”: mașina virtuală.
**Nu rula nimic de aici pe laptopul tău**, doar în mașina virtuală (regula 1 din `CLAUDE.md`).

Testele automate de pe GitHub au verificat deja aproape totul. Tu verifici ce nu poate face un robot:
că setările chiar se văd în Windows și că mesajele sunt clare.

---

## 1. Pregătește fișierele (pe laptop)

Ai nevoie de două fișiere, deja puse într-un folder:
`Documents\pc-optimizer\dist\vm-test\` → `pco.exe` și `vm-snapshot.ps1`.

Dacă folderul lipsește sau între timp s-a schimbat codul, refă-l. În PowerShell, pe laptop:

```powershell
cd $HOME\Documents\pc-optimizer
cargo build --release -p pco-cli
New-Item -ItemType Directory -Force dist\vm-test
Copy-Item target\release\pco.exe, scripts\vm-snapshot.ps1 dist\vm-test\
```

## 2. Pornește mașina virtuală și fă un checkpoint

1. Pornește mașina virtuală cu Windows 11 (vezi `START-AICI.md`, punctul 5).
2. În **Hyper-V Manager**: click dreapta pe mașina virtuală → **Checkpoint**. Numește-l `inainte-de-test`.
   La final revii la el și mașina e ca nouă.

## 3. Copiază fișierele în mașina virtuală

1. În mașina virtuală, deschide **File Explorer** și creează folderul `C:\pco`.
2. Pe laptop, selectează cele două fișiere din `dist\vm-test`, apasă **Ctrl+C**.
3. În mașina virtuală, în `C:\pco`, apasă **Ctrl+V**.
   (Merge dacă mașina e deschisă în „Enhanced session”, cu meniul *View → Enhanced session* bifat.)

## 4. Deschide PowerShell ca administrator (în mașina virtuală)

Start → scrie „PowerShell” → click dreapta → **Run as administrator** → **Da**. Apoi:

```powershell
cd C:\pco
Start-Transcript C:\pco\jurnal-test.txt
```

`Start-Transcript` salvează tot ce apare în fereastră în `jurnal-test.txt`, ca să mi-l poți da la final.

---

## Testul A — doar citire (nimic nu se schimbă)

```powershell
.\pco.exe --version
.\pco.exe --lang ro list
.\pco.exe --lang ro plan gaming.game-mode-on input.sticky-keys-hotkey-off
.\pco.exe --lang ro apply gaming.game-dvr-off --dry-run
.\pco.exe --lang ro status
```

**Ce trebuie să vezi:**
- `--version` → `pco 0.0.1`.
- `list` → un tabel cu 6 rețete. `services.fax-disabled` apare „nu se aplică: serviciul „Fax” nu e instalat” (normal pe Windows 11).
- `plan` → pentru fiecare rețetă, linii de forma `cheie: valoare veche → valoare nouă`.
- `--dry-run` → la final „Simulare: nu am schimbat nimic.”
- `status` → „Nicio sesiune în jurnal.”

## Testul B — aplicare și anulare exactă

**B1. Fotografia de dinainte:**

```powershell
powershell -ExecutionPolicy Bypass -File C:\pco\vm-snapshot.ps1 -Out C:\pco\inainte
```

**B2. Aplică toate rețetele:**

```powershell
.\pco.exe --lang ro apply gaming.game-mode-on gaming.game-dvr-off input.sticky-keys-hotkey-off privacy.advertising-id-off windows.taskbar-end-task services.fax-disabled
```

Trebuie să vezi linii cu `+ ...: aplicat` (sau `= ...: era deja aplicat`), `- services.fax-disabled: nu se aplică...`
și la final „Unele modificări au efect abia după: delogare și logare din nou.”

```powershell
.\pco.exe --lang ro list
```

Rețetele aplicate apar acum „aplicat”.

**B3. Anulează tot și compară:**

```powershell
.\pco.exe --lang ro undo --all
powershell -ExecutionPolicy Bypass -File C:\pco\vm-snapshot.ps1 -Out C:\pco\dupa -CompareWith C:\pco\inainte
```

Trebuie să scrie cu verde: **`IDENTICAL: every key is exactly as before.`**
Dacă scrie cu roșu `DIFFERENT`, nu repara nimic: notează și continuă.

> Fotografia o facem **înainte** să deschizi Setările, pentru că și Windows scrie singur în aceste chei
> când deschizi pagina de Jocuri. De aceea verificarea „cu ochii” vine separat, la B4.

**B4. Verificarea cu ochii.** Aplică din nou:

```powershell
.\pco.exe --lang ro apply gaming.game-mode-on gaming.game-dvr-off input.sticky-keys-hotkey-off privacy.advertising-id-off windows.taskbar-end-task
```

Apoi **delogare și logare** (Start → poza ta → *Sign out*, apoi intră din nou). Verifică:

| Ce | Unde | Trebuie să fie |
| --- | --- | --- |
| Game Mode | Setări → Gaming → Game Mode | Activat |
| Captura în fundal | Setări → Gaming → Captures → „Record what happened” | Dezactivat |
| ID de publicitate | Setări → Privacy & security → General → primul comutator | Dezactivat |
| Sticky Keys | apasă **Shift de 5 ori**, repede | **Nu** apare fereastra Sticky Keys |
| End Task | deschide Notepad, click dreapta pe iconița lui din taskbar | Apare „End task” |

Redeschide PowerShell ca administrator, apoi:

```powershell
cd C:\pco
.\pco.exe --lang ro undo --all
```

Încă o dată **delogare și logare**. Acum Shift de 5 ori **trebuie** să deschidă fereastra Sticky Keys (apasă *No*),
iar „End task” nu mai apare la click dreapta în taskbar.

## Testul C — întrerupere în mijlocul aplicării

Simulăm o pană de curent: `pco` se oprește brusc după prima modificare.

```powershell
cd C:\pco
Start-Transcript C:\pco\jurnal-test.txt -Append
powershell -ExecutionPolicy Bypass -File C:\pco\vm-snapshot.ps1 -Out C:\pco\inainte-c
$env:PCO_TEST_STOP_AFTER_CHANGES = "1"
.\pco.exe --lang ro apply gaming.game-dvr-off privacy.advertising-id-off
$LASTEXITCODE
```

Trebuie să vezi `PCO_TEST_STOP_AFTER_CHANGES: stopping now, on purpose (test)` și apoi `99`.

```powershell
Remove-Item Env:PCO_TEST_STOP_AFTER_CHANGES
.\pco.exe --lang ro status
powershell -ExecutionPolicy Bypass -File C:\pco\vm-snapshot.ps1 -Out C:\pco\dupa-c -CompareWith C:\pco\inainte-c
```

Trebuie să vezi:
- „O sesiune anterioară (#…) a fost întreruptă. Am anulat-o automat: 1 modificări restaurate.”
- în listă, sesiunea apare „anulată”;
- la final, cu verde, **`IDENTICAL`**.

## Testul D — gărzile

**D1. Joc pornit.** Facem un „joc” fals: un program inofensiv redenumit `cs2.exe`, care doar așteaptă 10 minute.

```powershell
Copy-Item C:\Windows\System32\timeout.exe C:\pco\cs2.exe
Start-Process C:\pco\cs2.exe -ArgumentList 600
.\pco.exe --lang ro apply gaming.game-mode-on
$LASTEXITCODE
```

Trebuie: „Închide jocul întâi: Counter-Strike 2 (cs2.exe)” și `2`. Apoi:

```powershell
Stop-Process -Name cs2
Stop-Transcript
```

**D2. Fără drepturi de administrator.** Deschide un PowerShell **normal** (nu ca administrator):

```powershell
cd C:\pco
.\pco.exe --lang ro apply privacy.advertising-id-off
$LASTEXITCODE
```

Trebuie: mesajul „Pentru asta e nevoie de drepturi de administrator…” și `2`.

---

## 5. Ce îmi trimiți

1. Copiază `C:\pco\jurnal-test.txt` din mașina virtuală pe laptop (Ctrl+C / Ctrl+V), în `Documents\pc-optimizer\`.
2. Spune-mi, pe scurt, pentru fiecare test: **merge / nu merge**, plus orice ți s-a părut neclar
   (inclusiv la tabelul din B4).

Eu citesc jurnalul și repar ce e nevoie.

## 6. Curățenie

În **Hyper-V Manager**: click dreapta pe checkpoint-ul `inainte-de-test` → **Apply**. Mașina virtuală revine exact
cum era înainte de test.
