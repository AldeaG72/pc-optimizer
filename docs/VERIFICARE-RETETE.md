# Verificarea rețetelor în documentația Microsoft

Verificat pe 2026-11-20 (Etapa 0, sarcina 0.9). Regula: `verified: true` doar când **cheia exactă**
apare în documentația Microsoft **și** testul automat apply → undo trece în CI (`windows-latest`).
**Testul din CI** (`real-recipes`, pe `windows-2025`, build 26100) a trecut pe 2026-11-20: toate cele 5 rețete
de registru au fost aplicate și anulate exact, pe un Windows real; acțiunile pe servicii au trecut pe un serviciu
de test creat și șters de CI. Prima rulare completă: <https://github.com/AldeaG72/pc-optimizer/actions/runs/44813962362>.

| Rețetă | Ce e documentat oficial | Ce nu e | Decizie |
| --- | --- | --- | --- |
| `gaming.game-mode-on` | `HKCU\Software\Microsoft\GameBar` → `AutoGameModeEnabled` (DWORD 0/1), legat de comutatorul Game Mode din Setări ([Reference for Windows 11 settings](https://learn.microsoft.com/en-us/windows/apps/develop/settings/settings-windows-11)) | — | Confirmat în documentație și în CI: **`verified: true`**. |
| `gaming.game-dvr-off` | Doar politica `AllowGameDVR` (HKLM, pentru tot PC-ul) ([ApplicationManagement Policy CSP](https://learn.microsoft.com/en-us/windows/client-management/mdm/policy-csp-applicationmanagement)) | `GameDVR_Enabled` și `AppCaptureEnabled` (HKCU) apar doar în răspunsuri din comunitate ([Microsoft Q&A](https://learn.microsoft.com/en-us/answers/questions/3738984/how-to-disable-xbox-game-bar)) | Păstrăm HKCU (ca butonul din Setări, fără admin și fără mesajul „gestionat de organizație”). Rămâne `false`. |
| `privacy.advertising-id-off` | `HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\AdvertisingInfo` → `Enabled` = 0, secțiunea 18.1 ([Manage connections…](https://learn.microsoft.com/en-us/windows/privacy/manage-connections-from-windows-operating-system-components-to-microsoft-services)) | Valoarea HKCU, pe care o scrie butonul din Setări | Scriem **ambele**. Rețeta cere acum drepturi de admin. Rămâne `false` din cauza valorii HKCU. |
| `input.sticky-keys-hotkey-off` | Biții: `SKF_HOTKEYACTIVE` = 0x4; 510 (0x1FE) fără bitul ăsta = 506 (0x1FA) ([STICKYKEYS](https://learn.microsoft.com/windows/win32/api/winuser/ns-winuser-stickykeys)) | Locul din registru (`HKCU\Control Panel\Accessibility\StickyKeys`, `Flags`, text) | Se aplică **doar dacă** valoarea e cea implicită `510` (`only_if`), ca să nu stricăm alte opțiuni alese de utilizator. Rămâne `false`. |
| `services.fax-disabled` | — (ghidul Windows Server 2016 citat înainte nu conține serviciul Fax) | Numele serviciului `Fax` apare doar în răspunsuri din comunitate ([Microsoft Q&A](https://learn.microsoft.com/en-us/answers/questions/2544470/windows-fax-service-not-staying-started)) | Scoasă din profilurile Recomandate/Laptop: rămâne rețetă de test. Rămâne `false`. |
| `windows.taskbar-end-task` | Comutatorul „End Task” din Setări › Sistem › Avansat ([Advanced Windows Settings](https://learn.microsoft.com/en-us/windows/advanced-settings/)) | Valoarea din registru și build-ul minim 22631 | Sursa-placeholder a fost înlocuită. Rămâne `false`; testul din mașina virtuală confirmă valoarea. |

## Schimbări făcute în cod

- **`only_if`** (nou, opțional, la acțiunile de registru): aplică doar dacă valoarea actuală e cea dată.
  Altfel rețeta „nu se aplică pe acest PC”, cu motivul afișat. Folosit la Sticky Keys.
- Versiunile rețetelor schimbate au crescut la 2: ID de publicitate, Sticky Keys, Fax.

## De făcut mai târziu

- Sticky Keys: schimbarea doar a bitului (sau prin `SystemParametersInfo(SPI_SETSTICKYKEYS)`), ca să meargă
  și când utilizatorul a schimbat alte opțiuni.
- O rețetă care devine documentată oficial poate trece pe `true` după testul din CI.
