# Testul proprietarului — Etapa 3 (prima beta), în Windows Sandbox

**Durată:** ~40 de minute. **Unde:** în **Windows Sandbox**, nu direct pe laptop (CLAUDE.md, regula 1).
Windows Sandbox e un Windows 11 de unică folosință, inclus în Windows 11 Pro: tot ce faci acolo dispare când
îl închizi, laptopul nu e atins.

## 1. Pornește Windows Sandbox (o singură dată)

1. Apasă **Start**, scrie `PowerShell`, clic dreapta pe **Windows PowerShell › Run as administrator**.
2. Copiază și rulează:

   ```powershell
   Enable-WindowsOptionalFeature -Online -FeatureName Containers-DisposableClientVM -All
   ```

3. Când cere, repornește laptopul (scrie `Y`).
4. După restart: **Start**, scrie `Windows Sandbox`, deschide-l.

## 2. Copiază programul de instalare în Sandbox

1. Pe laptop, descarcă `PC Optimizer_…_x64-setup.exe` din
   [Releases](https://github.com/AldeaG72/pc-optimizer/releases) (versiunea marcată **Pre-release**).
2. Copiază fișierul (Ctrl+C) și lipește-l (Ctrl+V) pe desktopul din fereastra Sandbox.

## 3. Ce verifici (bifează în minte; notează ce nu merge)

1. **Instalarea:** dublu-clic pe Setup. Windows poate spune „Windows protected your PC” → **More info › Run
   anyway** (beta-ul nu e semnat încă). Acceptă UAC. Aplicația pornește.
2. **Turul:** 4 ecrane, alegi limba, apeși „Sunt de acord”.
3. **Acasă:** vezi procesorul, memoria, Windows-ul; problemele găsite au „Rezolvă” (în Sandbox unele lipsesc,
   e normal: nu are placă video reală).
4. **Tweak-uri › Recomandate:** apasă **Recomandate**, apoi **Aplică tot**, acceptă UAC. Lista se bifează pe rând;
   la final, câte au reușit și câte au fost sărite (în Sandbox unele „nu se aplică” — e normal).
5. **Pornire:** deschide categoria „Pornire”; oprește un program (dacă are), apasă „Aplică”.
6. **Curățare:** bifează „Fișierele tale temporare”, apasă „Curăță”, citește întrebarea, confirmă.
7. **Setări › Siguranță › Jurnal pentru suport › Exportă…:** salvează pe desktop și deschide fișierul: nu
   trebuie să apară numele utilizatorului din Sandbox („WDAGUtilityAccount”).
8. **Setări › Depanare:** apasă „Sunet” la Panouri Windows (se deschide panoul de sunet). Nu rula reparațiile
   în Sandbox (sunt lente și nu au rost acolo).
9. **Setări › Anulează tot:** confirmă; toate tweak-urile revin.
10. **Dezinstalarea:** Start › Settings › Apps › Installed apps › PC Optimizer › Uninstall. Trebuie să se
    termine fără mesaj de eroare.

Gata: închide Windows Sandbox (tot ce era acolo se șterge).

## 4. Ce îmi trimiți

- „Totul a mers” sau, pentru fiecare pas care n-a mers: numărul pasului și ce ai văzut (o captură de ecran ajută:
  Win+Shift+S).
- Dacă ceva a dat eroare: fișierul de la pasul 7.
