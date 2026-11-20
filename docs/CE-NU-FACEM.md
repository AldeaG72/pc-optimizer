# Ce nu facem și de ce

Pagina asta devine publică (link din aplicație și din README). Când cineva cere unul dintre
lucrurile de mai jos, răspunsul e linkul spre această pagină.

| Nu facem | De ce |
| --- | --- |
| Modificări în BIOS, overclocking, undervolting | Risc hardware; nu se pot anula din Windows. Aplicația doar detectează și explică (ex.: XMP/EXPO) |
| Oprirea Windows Defender, a firewall-ului, a UAC, a SmartScreen | Lasă PC-ul expus; câștig de performanță neglijabil |
| Oprirea completă a update-urilor | Lasă PC-ul fără patch-uri de securitate. Permitem doar amânarea update-urilor de funcții (Pro) |
| Blocarea prin fișierul hosts, reguli de firewall pentru servere Microsoft | Strică update-uri, Store, Xbox, activarea |
| Oprirea serviciilor de diagnostic esențiale, servicii trecute pe Manual în masă | Pot strica funcții Windows; modificăm doar servicii dintr-o listă sigură |
| „Registry cleaning” | Nu aduce niciun câștig măsurabil |
| Valori „magice” pentru NetworkThrottlingIndex, SystemResponsiveness, TcpAckFrequency | Efect zero sau neverificat pe Windows 10/11 moderne |
| Oprirea algoritmului Nagle | Jocurile moderne folosesc UDP; fără efect pe ping |
| Timer resolution global | Efect limitat pe Windows 11, poate crește consumul. Doar explicat |
| Tweak-uri `bcdedit` pentru HPET / platform clock | Des înrăutățesc frametime-ul |
| Dezactivarea paginefile-ului | Poate provoca crash-uri la jocurile care consumă multă memorie |
| Dezactivarea IPv6 / Teredo, „Prefer IPv4” | Poate strica Xbox Live și unele jocuri; fără câștig de ping dovedit |
| Dezinstalarea Edge, „șterge toate aplicațiile din Store” | Strică Widgets, Store și aplicații Windows. Edge e doar oprit în fundal |
| Anti-Lag din driverul AMD | Anti-Lag+ din driver a dus în 2023 la ban-uri VAC în CS2. Recomandăm Anti-Lag 2 din joc |
| Fullscreen Optimizations oprite global | Efect contestat pe Windows 11 |
| Prioritatea proceselor jocurilor, injectare în jocuri, overlay în joc | Poate fi confundat cu un cheat de anti-cheat |
| Descărcarea și instalarea de drivere | Doar avertizăm și dăm linkul oficial |
| Modificarea fișierelor de config ale jocurilor | Doar recomandăm setările |
| Imagini Windows personalizate (ISO), instalare de aplicații | Alt tip de produs |
| Telemetrie | Nu colectăm nimic |
