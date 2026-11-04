# Frontend CelLab

Interfață Angular pentru prezentarea simulatorului: alegerea unui model, setarea
parametrilor, validare, rulare, navigarea generațiilor și inspecția unei celule.

## Pornire

Folosește două terminale, pornind din rădăcina proiectului.

Backend, pe portul 3001:

```powershell
cd backend
$env:APP_ADDRESS = '127.0.0.1:3001'
cargo run
```

Frontend, pe portul 4201:

```powershell
cd frontend
npm install
npm start
```

Deschide `http://127.0.0.1:4201`. Porturile sunt separate de 3000 și 4200, folosite
de alte aplicații locale. Proxy-ul din `proxy.conf.json` trimite `/api/**` la
backend și elimină prefixul `/api`. Nu este necesară o configurare CORS pentru
acest flux local. La schimbarea portului backend, actualizează ținta proxy-ului.

`npm start` pornește serverul de dezvoltare. Un build publicat separat trebuie
servit împreună cu un proxy `/api` echivalent; configurația de dezvoltare nu este
inclusă automat în serverul de producție.

## Prezentare rapidă

1. Alege **Game of Life**, rulează și treci între generațiile 0, 1 și 2.
2. Alege **Difuzia căldurii** și urmărește valorile zecimale.
3. Alege **Ecosistem artificial** pentru a vedea agenții peste teren.
4. Selectează o celulă pentru tipul, proprietățile și agenții ei.
5. Dezactivează păstrarea istoricului: rezultatul arată generația finală, iar
   redarea este indisponibilă fiindcă nu există cadre intermediare.

Butonul de redare parcurge generațiile calculate, la intervale de 500 ms.
Nu recalculează simularea. Același model, seed și aceiași parametri reproduc
rezultatele. Schimbarea modelului golește rezultatul anterior și încarcă lumea
inițială a noului exemplu.

Configurațiile provin din `backend/examples`, prin
`GET /simulations/examples/{name}`. Numele și descrierile afișate sunt în
`simulator/examples.ts`. Cele 17 exemple folosesc același API.

## Structură

- `simulation.models.ts`: tipurile datelor schimbate cu Rust. Regulile sunt
  `unknown[]`: frontend-ul le transmite, iar backend-ul le interpretează.
- `simulation.service.ts`: cererile HTTP pentru exemple, sănătatea motorului,
  validare și rulare.
- `simulator/`: formularul reactiv, starea prin signals, istoricul și redarea.
- `world-grid/`: afișarea celulelor și agenților, culori, valori numerice și
  evenimentul de selectare a unei celule.
- `app.routes.ts`: încărcarea ecranului la accesarea rutei.

Interfața pornește din lumile exemplelor. Editarea lumii și descrierea în limbaj
natural sunt pași separați. Grila permite inspecție, fără modificarea celulelor.
Seed-ul introdus trebuie să fie un întreg între 0 și `Number.MAX_SAFE_INTEGER`,
pentru reprezentarea exactă în JavaScript; backend-ul acceptă în continuare `u64`.

## Verificare

```powershell
npm run build
npm test -- --watch=false
```

Testele verifică încărcarea exemplului, parametrii trimiși, ordinea validare–rulare,
erorile backend, navigarea și modul cu doar generația finală. `axe-core` este o
dependență de dezvoltare. Testele automate verifică etichetele și atributele ARIA;
contrastul și aspectul responsive se verifică într-un browser real. Biblioteca
nu este inclusă în aplicația de producție. Stilurile sunt CSS simplu; nu există
o bibliotecă UI.
