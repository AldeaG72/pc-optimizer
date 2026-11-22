# Semnarea digitală a programului de instalare (Etapa 3, pasul 3.16)

**De ce:** fără semnătură, Windows arată la instalare „Windows protected your PC” (SmartScreen), iar unele
antivirusuri tratează programul cu suspiciune. SignPath Foundation semnează gratuit proiecte open-source.
Până la aprobare, beta-ul iese **nesemnat**, cu explicația din README.

## Ce faci tu (o singură dată, ~20 de minute)

1. Deschide <https://signpath.org/apply> și citește condițiile (pe scurt: licență open-source — avem GPL-3.0;
   cod public pe GitHub; programul construit automat în GitHub Actions — avem; o politică de semnare publicată
   în README — am pus-o; autentificare în doi pași pe contul GitHub).
2. Activează **autentificarea în doi pași** pe GitHub, dacă n-o ai: GitHub › Settings › Password and
   authentication › Two-factor authentication.
3. Completează formularul. Ce să scrii:
   - **Project name:** PC Optimizer (working title)
   - **Repository:** https://github.com/AldeaG72/pc-optimizer
   - **License:** GPL-3.0-only
   - **Description:** Open-source Windows 11 optimizer for competitive gamers. It only changes documented
     Windows settings, each one explained, sourced and exactly reversible; no telemetry.
   - **Build system:** GitHub Actions (`.github/workflows/ci.yml`, job „installer”)
   - **Artifact to sign:** the NSIS installer `PC Optimizer_<version>_x64-setup.exe` (and the executables in it)
   - Adresa de e-mail: a ta (nu o pune în cod sau în repo).
4. Trimite formularul și așteaptă răspunsul pe e-mail (poate dura câteva săptămâni).
5. Când vine aprobarea, spune-mi. Pașii de atunci (îi fac eu, cu tine la tastatură pentru parole):
   - instalezi aplicația **SignPath** pe repo din GitHub (link în e-mailul lor);
   - în repo › Settings › Secrets and variables › Actions adaugi secretul `SIGNPATH_API_TOKEN`
     (îl copiezi tu din contul SignPath; eu nu văd parole sau chei);
   - eu adaug în CI pasul care trimite programul de instalare la semnat și publică varianta semnată.

## Ce e deja pregătit

- Programul de instalare e construit numai în GitHub Actions, din codul public (job-ul „installer”).
- README are secțiunea „Politica de semnare” cerută de SignPath.
- Fără semnătură, README explică avertismentul SmartScreen și cum verifici că fișierul e cel bun.
