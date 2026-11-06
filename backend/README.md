# Backend morphix

Din directorul `backend`, pornește serverul cu `cargo run`. Adresa implicită este
`http://127.0.0.1:3000`; configurația se citește din variabilele de mediu și `.env`.

## Variantele implementării

- `codex/advanced-simulator` păstrează versiunea completă dinaintea simplificării,
  în commit-ul `212a45b`.
- `main` folosește un flux intern mai ușor de urmărit și patru exemple temporare.

Pe `main`, intenția de mișcare are un singur câmp, `Option<Movement>`, cu variantele
`Offset`, `FreeCell` și `RemovedNeighbor`. Simulatorul păstrează agentul, rezultatul
regulii și destinația într-un `AgentDecision`, definit în `src/simulator.rs`.
Rezolvarea capturilor și a deplasărilor parcurge această listă de decizii.

Definiția automatului și lumea inițială sunt validate la intrarea în `run()` sau
`step()`. În timpul unei rulări, simulatorul folosește funcțiile interne
`read_validated()`, `apply_rules_validated()` și `apply_actions_validated()` pentru
datele deja verificate. Valorile produse de acțiuni sunt verificate în continuare,
iar fiecare lume rezultată este verificată structural: grilă, poziții și ID-uri.
Funcțiile publice `read()`, `apply_rules()` și `apply_actions()` își păstrează
verificările de intrare pentru utilizarea lor independentă.

Alegerea celulelor libere, pregătirea nașterilor și verificarea limitelor
istoricului sunt scrise cu bucle și condiții explicite. Testele existente
verifică în continuare comportamentul și contractul JSON.

Numărarea vecinilor și citirea generațiilor din SQLite folosesc de asemenea bucle
și ramuri explicite, pentru a putea urmări ușor fiecare operație.

Pentru compararea implementărilor, din rădăcina proiectului:

```powershell
git diff codex/advanced-simulator..main -- backend/src
```

## Exemple temporare

Backendul expune aceleași patru exemple ca formularul de creare:

| Fișier din `examples/` | Ce verifică |
| --- | --- |
| `game-of-life.json` | Celule cu stări discrete |
| `forest-fire.json` | Reguli probabilistice |
| `diffusion.json` | Proprietăți numerice cu valori zecimale |
| `artificial-ecosystem.json` | Agenți mobili, energie, captură și reproducere |

Toate folosesc același evaluator de condiții, expresii și acțiuni. Alegerea unui
exemplu este temporară: serviciul AI va genera ulterior definiția automatului și
lumea inițială pornind de la un prompt.

`tests/catalog.rs` verifică aceste patru configurații. `tests/common/mod.rs`
conține date compacte folosite exclusiv de teste pentru mecanismele motorului,
precum deplasarea, captura și transferul numeric între celule.

## Endpoint-uri

- `GET /health`: verifică legătura cu baza de date; răspunde cu `ok` sau HTTP 503.
- `GET /simulations`: listează simulările fără configurații și istorice complete.
  Parametri: `page` (implicit 1), `page_size` (implicit 20, maximum 100), `search`
  (fragment literal din nume), `sort` (`created_at`, `name`, `generations`) și
  `order` (`asc`, `desc`; implicit `desc`). Răspunsul conține `items`, `total`,
  `page`, `page_size`. Fiecare element are `id`, `name`, `created_at`, `width`,
  `height`, `generations`, `save_history`. Paginile în afara rezultatelor sunt
  goale; parametrii invalizi primesc HTTP 400 cu un mesaj JSON. Căutarea și
  sortarea după nume ignoră diferențele de majuscule ASCII.
- `GET /simulations/examples/{name}`: întoarce configurația completă a unui exemplu,
  de exemplu `game-of-life`, `diffusion` sau `forest-fire`. Numele necunoscute produc
  HTTP 404. Frontend-ul folosește fișierele originale prin acest endpoint.
- `POST /simulations/validate`: verifică o cerere înainte de rulare și răspunde cu
  `{ "valid": true }`. Nu calculează generații și nu salvează în SQLite.
- `POST /simulations/run`: primește JSON cu `automaton`, `initial_world`, `generations`
  și `seed`, plus opțiunea `save_history`; răspunde cu
  `{ "id": 1, "seed": 42, "history": [{ "generation": 0, "world": ... }, ...] }`.
- `GET /simulations/{id}`: recuperează simularea salvată, cu `id`, `created_at`,
  `request` (configurația completă) și `history` (generațiile păstrate).
- `DELETE /simulations/{id}`: șterge simularea și întoarce HTTP 204 fără corp.
  Cheile străine SQLite elimină atomic generațiile și checkpoint-ul asociat.
  Un ID inexistent primește 404, un ID incorect formatat primește 400, iar o eroare
  de stocare primește 500 cu mesaj JSON. Un pas calculat înainte de ștergere nu
  poate recrea înregistrarea, deoarece salvarea lui necesită simularea existentă.
- `GET /simulations/{id}/generations/{generation}`: recuperează o generație salvată,
  în forma `{ "generation": 1, "world": ... }`.
- `POST /simulations/{id}/step`: primește `{ "generation": 6 }`, numărul ultimei
  generații cunoscute, și calculează o singură generație nouă. Răspunsul este
  `{ "generation": 7, "world": ... }`. Salvează rezultatul și actualizează numărul
  generațiilor din lista simulărilor. O cerere pornită de la o generație depășită
  primește HTTP 409; un ID inexistent primește 404.

Frontendul creează o simulare cu `generations: 0` și `save_history: true`, apoi
apelează `/step` la fiecare avansare dincolo de istoricul calculat. Acest flux nu
are limita cumulativă de 1.000 de generații sau de 100.000 de unități de istoric:
limita de stare/proprietăți se aplică unei singure lumi, iar istoricul rămâne în SQLite.
Vechiul `/run` păstrează limitele unui lot calculat și returnat într-o singură cerere.

Tabela `simulation_checkpoints`, creată la pornire prin scriptul existent,
păstrează poziția generatorului aleatoriu și ultimul ID de agent alocat.
`ChaCha12Rng` este algoritmul folosit de `StdRng` din versiunea curentă de `rand`;
este folosit direct pentru a-i putea salva și restaura poziția. Pașii separați
reproduc aceeași evoluție ca rularea continuă, inclusiv după repornirea serverului.
Pentru simulările vechi, fără checkpoint, acesta este recuperat o singură dată
prin reluarea configurației originale, cu același seed, fără acumularea istoricului.

Un pas citește doar ultima lume și checkpoint-ul, apoi execută calculul în
`spawn_blocking`. Noua lume, checkpoint-ul și numărul generației sunt scrise
într-o singură tranzacție. Dacă un alt apel a avansat deja simularea, actualizarea
este refuzată; dacă regulile sau salvarea eșuează, starea anterioară rămâne intactă.
Simulările vechi cu `save_history: false` păstrează în continuare numai ultima lume.

Fiecare element din `history` conține numărul generației în `generation` și lumea
completă în `world`, inclusiv celulele și agenții.

Citirea JSON folosește opțiunea `float_roundtrip` din `serde_json`, astfel încât
valorile zecimale `f64` din istoric să fie recuperate exact după salvarea în SQLite.

- `save_history: true` (implicit dacă lipsește): pentru N pași sunt returnate N + 1
  stări, numerotate de la 0 la N. `history[0].world` este lumea inițială.
- `save_history: false`: se calculează toți pașii, dar este returnat un singur element,
  cu `generation: N` și lumea finală în `world`. Lumile intermediare sunt eliberate
  pe parcurs, fără acumularea unui istoric complet.
- Pentru zero pași, ambele moduri returnează lumea inițială, cu `generation: 0`.

De exemplu, cu `generations: 20` și `save_history: false`, `history[0].generation`
este 20, iar `history[0].world` este starea de după 20 de pași. Numărul generației
se citește din câmpul explicit, nu se deduce din poziția în listă.

Această opțiune controlează stările reținute în memorie, salvate în SQLite și
returnate în răspuns. Calculul rulează într-un worker separat, prin `spawn_blocking`.
Alegerea istoricului nu modifică
evoluția, secvența aleatoare sau alocarea ID-urilor agenților: pentru aceeași configurație
și același seed, lumea finală este identică în ambele moduri.

## Validarea înainte de rulare

`POST /simulations/validate` primește aceeași structură ca `/simulations/run`:
`automaton`, `initial_world`, `generations`, `seed` și opțional `save_history`.
Astfel, aceeași configurație poate fi verificată, corectată în interfață și apoi
trimisă la rulare. Inclusiv numărul de pași și alegerea istoricului sunt validate.

Endpoint-ul reutilizează `SimulationRequest::validate_limits()` și
`AutomatonDefinition::validate_world()`. Verifică limitele cererii, tipurile,
proprietățile, referințele și combinațiile regulilor, vecinătatea, marginea și
lumea inițială. Nu evaluează regulile, nu consumă numere aleatoare și nu alocă ID-uri
de simulare. Verificarea lumii este executată prin `spawn_blocking`, iar handler-ul
nu folosește baza de date. Formatele JSON existente și cererile din `examples/`
sunt acceptate direct.

La succes primește HTTP 200 cu `{ "valid": true }`. La eroare primește
`{ "error": "..." }`, cu aceleași coduri HTTP ca ruta de rulare: 400 pentru JSON
incorect, 415 pentru un tip de conținut nepotrivit, 413 pentru un corp prea mare
și 422 pentru schema sau configurația invalidă. Verificarea se oprește la prima
eroare. Erorile lumii sau definiției au prefixul `Generation 0`, la fel ca la rulare.

Acesta este un control static al configurației și al lumii inițiale. `{ "valid": true }`
nu garantează succesul tuturor generațiilor: o expresie dinamică poate produce
împărțire la zero, o probabilitate poate ieși din `[0, 1]`, populația poate depăși
limitele sau pot apărea conflicte de scriere a terenului. Aceste situații sunt
verificate când se execută simularea. Nici acțiunile primei generații nu sunt
evaluate de ruta de validare.

Din `backend`, cu serverul pornit:

```powershell
$body = Get-Content -Raw examples/artificial-ecosystem.json
$validation = Invoke-RestMethod -Method Post `
    -Uri 'http://127.0.0.1:3000/simulations/validate' `
    -ContentType 'application/json' -Body $body
$validation.valid

$result = Invoke-RestMethod -Method Post `
    -Uri 'http://127.0.0.1:3000/simulations/run' `
    -ContentType 'application/json' -Body $body
```

`SimulationValidationResponse` se află în `src/models/simulation.rs` și definește
doar răspunsul de succes. Erorile folosesc `ErrorResponse`, deja utilizat de API.
`tests/api.rs` verifică validarea, erorile, absența scrierilor în SQLite și absența
execuției regulilor. `tests/catalog.rs` trimite cele patru configurații la
validare înainte să le ruleze și să le recupereze din SQLite.

## Salvarea și recuperarea simulărilor

La pornire se deschide baza configurată prin `DATABASE_URL` (implicit
`sqlite://cellab.db`). Fișierul este creat dacă lipsește. Scriptul inclus în program,
`migrations/0001_create_simulations.sql`, inițializează cele două tabele cu
`CREATE TABLE IF NOT EXISTS`, fără a șterge datele existente. În această versiune
este un script de inițializare; nu există încă un sistem de migrații versionate.

- `simulations`: `id` generat de SQLite, data creării în UTC și `request_json`,
  configurația completă a experimentului. Aceasta include definiția automatului,
  lumea inițială, numărul de pași, seed-ul și alegerea istoricului.
- `simulation_generations`: `simulation_id`, numărul generației și `world_json`,
  lumea completă cu celule și agenți. Perechea simulare–generație este unică.

Configurația și lumile sunt JSON. Seed-ul `u64` este păstrat în JSON, inclusiv
valorile care depășesc intervalul întregilor SQLite cu semn. ID-ul simulării este
un `i64`, separat de ID-urile agenților, care rămân `u64`.

Fluxul este: validare, calcul, salvare într-o tranzacție, răspuns cu ID și rezultat.
`src/simulation_store.rs` conține operațiile SQL. Dacă o inserare eșuează, se anulează
întreaga salvare, inclusiv generațiile deja inserate. O eroare de simulare nu creează
înregistrări în baza de date; o eroare de salvare produce HTTP 500.

Cu `save_history: false`, tabelul generațiilor conține numai generația finală.
Lumea inițială rămâne disponibilă în `request.initial_world`. Cererea unei generații
intermediare nepăstrate produce HTTP 404; ruta de citire nu o recalculează.

Rulări distincte primesc ID-uri distincte, chiar dacă au aceeași configurație și
același rezultat. Datele pot fi recuperate după repornirea serverului folosind
același fișier SQLite. Configurația citită este serializată din modelele Rust:
de exemplu, o probabilitate trimisă numeric este returnată ca expresie `constant`,
iar `save_history` omis este returnat explicit ca `true`.

## Exemplu Game of Life

Fișierul `examples/game-of-life.json` conține o cerere completă cu un oscilator de
trei celule. În PowerShell, din `backend`:

```powershell
$result = Invoke-RestMethod -Method Post `
    -Uri 'http://127.0.0.1:3000/simulations/run' `
    -ContentType 'application/json' `
    -Body (Get-Content -Raw examples/game-of-life.json)

$result.history[1].world.cells

$saved = Invoke-RestMethod -Uri "http://127.0.0.1:3000/simulations/$($result.id)"
$saved.request.initial_world

$generation = Invoke-RestMethod `
    -Uri "http://127.0.0.1:3000/simulations/$($result.id)/generations/1"
$generation.world.cells
```

Generațiile 0 și 2 conțin linia orizontală, iar generația 1 conține linia verticală.

Variantele folosesc `kind` în JSON, de exemplu `{ "kind": "set_type", "type_id": 1 }`.
Condițiile și expresiile care au argumente folosesc și `args`, de exemplu
`{ "kind": "constant", "args": 3 }` sau
`{ "kind": "add", "args": [{ "kind": "property", "args": "energy" }, { "kind": "constant", "args": 1 }] }`.
Ținta unei reguli este `"cell"` sau `"agent"`. Câmpurile necunoscute sunt respinse.

## Exemplu de difuzie

`examples/diffusion.json` conține o cerere completă cu o grilă de 3 × 3 celule.
Există un singur tip, `material` (ID 0), cu proprietatea numerică `temperature` în
intervalul `[0, 100]`. `integer_only: false` permite temperaturi zecimale.
Tipul celulelor rămâne același; evoluează valoarea proprietății. Nu există agenți.

La fiecare pas, toate celulele folosesc temperaturile din generația curentă:

```text
T_nou = (T_curent + T_nord + T_sud + T_vest + T_est) / 5
```

Vecinătatea este `[[-1, 0], [1, 0], [0, -1], [0, 1]]`. Regula are condiția
`always`, probabilitatea 1 și o acțiune `set_property`. Expresia acesteia combină
`property`, `sum_neighbors`, `add` și `divide`, deja disponibile în motor.
Numitorul 5 reprezintă celula curentă plus cele patru offset-uri din această
configurație. Dacă schimbi numărul vecinilor pentru a calcula media tuturor lor,
trebuie să adaptezi și numitorul.

Marginea `wrap` conectează sus cu jos și stânga cu dreapta. Pentru această medie
simetrică, suma temperaturilor se păstrează, în limita erorilor de calcul `f64`.
Modelul presupune celule identice, cu aceeași capacitate termică; în aceste condiții
suma temperaturilor este proporțională cu energia termică totală.
Din temperatura 100 în centru și 0 în rest, cererea produce:

```text
Generația 0       Generația 1       Generația 2       Generația 3
  0   0   0        0  20   0        8  12   8       9.6  12  9.6
  0 100   0       20  20  20       12  20  12        12 13.6  12
  0   0   0        0  20   0        8  12   8       9.6  12  9.6
```

Suma este 100 în fiecare generație. Pentru multe iterații, valorile tind spre
media inițială, `100 / 9`, aproximativ `11.1111` în fiecare celulă. Este o regulă
discretă de difuzie pentru proiect, fără parametri fizici precum conductivitatea
sau durata pasului.

Cu o margine `fixed` având temperatura 0, vecinii din afara grilei sunt un mediu
rece, iar modelul poate pierde căldură. După doi pași, suma din grilă este 84.
Conservarea descrisă mai sus se referă la configurația circulară din exemplu.

Din `backend`, cu serverul pornit:

```powershell
$result = Invoke-RestMethod -Method Post `
    -Uri 'http://127.0.0.1:3000/simulations/run' `
    -ContentType 'application/json' `
    -Body (Get-Content -Raw examples/diffusion.json)

$result.history[3].world.cells | ForEach-Object {
    ($_.properties.temperature -join ' ')
}
```

## Citirea celulei curente

O regulă poate distinge obiectul care o execută de celula terenului din poziția lui:

- `Property("energy")` citește proprietatea obiectului: energia agentului, într-o
  regulă pentru agenți.
- `CellProperty("food")` citește proprietatea celulei curente. Pentru un agent este
  celula de sub el; pentru o regulă de celulă este chiar celula care execută regula.
- `TypeIs(1)` verifică tipul obiectului, iar `CellTypeIs(0)` verifică tipul celulei
  curente. Tipurile rămân cele declarate în `object_types`.

În JSON, cele două operații noi se scriu astfel:

```json
{ "kind": "cell_property", "args": "food" }
```

```json
{ "kind": "cell_type_is", "args": 0 }
```

Exemplu de regulă, presupunând că tipul 0 declară `food`, iar tipul 1 declară `energy`:

```json
{
  "target": "agent",
  "condition": {
    "kind": "all",
    "args": [
      { "kind": "type_is", "args": 1 },
      { "kind": "cell_type_is", "args": 0 },
      {
        "kind": "compare",
        "args": {
          "left": { "kind": "cell_property", "args": "food" },
          "operator": { "kind": "greater" },
          "right": { "kind": "constant", "args": 0 }
        }
      }
    ]
  },
  "probability": 1,
  "actions": [
    {
      "kind": "set_property",
      "property": "energy",
      "value": {
        "kind": "add",
        "args": [
          { "kind": "property", "args": "energy" },
          { "kind": "cell_property", "args": "food" }
        ]
      }
    }
  ]
}
```

Un agent cu energie 10 pe o celulă cu hrană 3 ajunge la energie 13, dacă starea
rezultată respectă limitele tipului. Regula citește hrana; modifică numai energia
agentului. Hrana rămâne neschimbată dacă celula nu are o regulă proprie care o modifică.
Pentru consumul efectiv se adaugă `SetCellProperty`, descrisă mai jos.

Verificarea `CellTypeIs(0)` precede citirea proprietății. Într-un `All`, o condiție
falsă oprește evaluarea ramurilor următoare, astfel încât un agent aflat pe alt tip
de teren poate sări citirea lui `food`. Dacă proprietatea este citită pe o celulă
care nu o are, se întoarce eroarea `Current cell is missing property 'food'`.
Nu se presupune valoarea zero și nu se folosește proprietatea agentului în locul ei.

Validarea statică verifică toate ramurile: ID-ul din `CellTypeIs` trebuie să fie
declarat, iar numele din `CellProperty` trebuie să existe pe cel puțin un tip.
Prezența proprietății pe celula efectivă se verifică la evaluare. Citirile celulei
pot apărea în condiții, probabilități, modificări de stare, offset-uri și proprietățile
agenților creați. Probabilitățile calculate trebuie să rămână în intervalul `[0, 1]`.

Toate citirile folosesc celula din lumea de la începutul pasului și poziția inițială
a agentului. O regulă proprie a celulei, `SetType`, o atribuire anterioară sau o
deplasare a agentului nu schimbă datele citite în acel pas. La pasul următor se
folosește celula din noua poziție. Vecinătatea nu trebuie să conțină `(0, 0)`;
citirea celulei curente funcționează inclusiv pentru o vecinătate goală.

În Rust, evaluarea primește acum explicit cele trei surse de date:

```rust
expression.evaluate(current, cell, neighbors)
condition.evaluate(current, cell, neighbors)
apply_actions(actions, current, cell, neighbors, automaton, rng)
apply_rules(target, current, cell, neighbors, automaton, rng)
```

`current` și `cell` sunt referințe `&ObjectState`. Pentru o regulă de celulă, ambele
indică aceeași stare inițială; pentru o regulă de agent, simulatorul transmite
`agent.state` și `world.cells[row][col]`. Celula este transmisă și prin operațiile
aritmetice și condițiile compuse, astfel încât o expresie imbricată citește aceleași date.

## Citirea unui vecin după offset

`NeighborTypeIs` verifică tipul celulei de la un anumit offset:

```json
{ "kind": "neighbor_type_is", "args": { "offset": [0, 1], "type_id": 1 } }
```

Aceasta verifică dacă vecinul din dreapta are tipul 1. `NeighborProperty` citește
o proprietate numerică a celulei alese:

```json
{ "kind": "neighbor_property", "args": { "offset": [0, -1], "property": "temperature" } }
```

Expresia citește temperatura vecinului din stânga și poate fi folosită în calcule,
comparații, probabilități și acțiuni, ca orice altă expresie.

Offset-ul are forma `[diferență_rând, diferență_coloană]`, cu două numere întregi
`i32`. Trebuie să existe în `neighborhood.offsets`; referințele nedeclarate sunt
respinse inclusiv pentru zero pași, probabilitate zero sau ramuri neexecutate.
`[0, 0]` nu este un vecin: pentru celula curentă există `CellProperty` și `CellTypeIs`.

Ordinea offset-urilor nu schimbă celula selectată. `Wrap` înfășoară coordonatele,
iar `Fixed` furnizează starea de margine pentru pozițiile din afara grilei. Pe o
grilă mică, mai multe offset-uri pot indica aceeași celulă, inclusiv celula curentă
după înfășurare; fiecare offset declarat rămâne disponibil separat.

Pentru agenți, offset-ul este relativ la poziția lor de la începutul pasului.
Toate citirile folosesc lumea veche, înaintea regulilor celulelor și deplasărilor.
Operațiile noi citesc celulele terenului; observarea agenților rămâne distinctă.

Proprietatea trebuie să fie declarată de cel puțin un tip. Dacă celula observată
nu o are, citirea produce o eroare care indică offset-ul și proprietatea. Pentru
terenuri cu proprietăți diferite se poate pune mai întâi `NeighborTypeIs`, urmat
de comparația proprietății într-un `All`; un tip nepotrivit oprește evaluarea.
Aceeași regulă se aplică stării de margine `Fixed`.

## Observarea agenților vecini

Vecinătatea poate furniza atât celule, cât și agenți. În Rust, `Neighborhood`
descrie offset-urile configurate, iar rezultatul concret al lui `read()` este:

```rust
pub struct Neighbors<'a> {
    pub offsets: Vec<(i32, i32)>,
    pub free_offsets: Vec<(i32, i32)>,
    pub cells: Vec<&'a ObjectState>,
    pub positions: Vec<Option<Position>>,
    pub agents: Vec<&'a Agent>,
}
```

Structura se află în `src/models/neighborhood.rs`. `cells` și `agents` conțin referințe
la datele observate. `offsets[i]` corespunde celulei `cells[i]`; `cell_at_offset()`
caută perechea cerută. `free_offsets` conține câte un offset pentru fiecare poziție
vecină distinctă fără agenți, din interiorul grilei și diferită de poziția curentă.
Lista păstrează ordinea configurată și este folosită pentru alegerea destinațiilor.
`positions[i]` este poziția reală a celulei observate sau `None` pentru exteriorul
unei margini fixe. Această corespondență permite recunoașterea alias-urilor de la
marginea circulară, chiar dacă o regulă folosește altă ordine a offset-urilor.
`'a` leagă durata referințelor de datele din care provin. `Neighbors`
este un rezultat temporar; formatul JSON al definiției, lumii și istoricului rămâne
cel existent. În evaluare, `neighbors` este acum `&Neighbors`, iar `Neighbors::default()`
reprezintă o observație cu toate listele goale.

`CountNeighbors` și `SumNeighbors` folosesc `neighbors.cells`. Două expresii noi
folosesc `neighbors.agents`:

```json
{ "kind": "count_neighbor_agents", "args": { "type_id": 2 } }
```

```json
{ "kind": "sum_neighbor_agents", "args": { "type_id": 2, "property": "energy" } }
```

Prima numără agenții vecini de tipul 2. A doua adună energia acestora. Dacă în
vecinătate sunt doi agenți de tipul 2 cu energii 5 și 7 și un agent de alt tip,
rezultatele sunt 2 și 12. Agenții de alte tipuri sunt ignorați de ambele expresii.
Fără agenți de tipul cerut, numărul și suma sunt zero.

Exemplu de regulă care actualizează numărul vecinilor de tipul 2, presupunând că
tipul 1 declară proprietatea `nearby` cu limite potrivite:

```json
{
  "target": "agent",
  "condition": { "kind": "type_is", "args": 1 },
  "probability": 1,
  "actions": [
    {
      "kind": "set_property",
      "property": "nearby",
      "value": { "kind": "count_neighbor_agents", "args": { "type_id": 2 } }
    }
  ]
}
```

Pentru condiția „există pradă în apropiere”, se compară această expresie cu zero:

```json
{
  "kind": "compare",
  "args": {
    "left": { "kind": "count_neighbor_agents", "args": { "type_id": 2 } },
    "operator": { "kind": "greater" },
    "right": { "kind": "constant", "args": 0 }
  }
}
```

Regulile de observare sunt:

- Se folosesc aceleași offset-uri configurate pentru celule și agenți. Sunt citite
  exact pozițiile indicate, inclusiv offset-uri mai mari decât o celulă.
- Poziția curentă este exclusă din vecinătatea agenților, inclusiv dacă un offset
  ajunge acolo prin `Wrap`. Sunt excluși toți agenții aflați în acea poziție.
- Fiecare agent de pe o poziție vecină este inclus o singură dată. Dacă mai multe
  offset-uri ajung la aceeași poziție prin `Wrap`, agentul nu este duplicat.
- Mai mulți agenți aflați pe aceeași poziție vecină sunt observați individual.
- Pentru celule se păstrează un rezultat pentru fiecare offset, inclusiv când
  înfășurarea ajunge de mai multe ori la aceeași celulă. Aceasta păstrează semantica
  regulilor de automate celulare existente.
- `Fixed` furnizează starea de margine pentru celulele din afara grilei; acea stare
  nu generează agenți. O listă goală de offset-uri produce ambele liste goale.
- Agenții observați păstrează ordinea din lista lumii. Această ordine este folosită
  și la însumarea proprietăților.

Toate citirile folosesc lumea de la începutul pasului: tipuri, proprietăți și poziții.
Un agent eliminat în acel pas este încă observabil din starea inițială; un copil
nou-creat este observabil începând cu pasul următor. Aceste date sunt disponibile
și regulilor pentru celule. Expresiile pot apărea în condiții, probabilități,
modificări de stare, deplasări, creări și modificări ale terenului.

Validarea verifică existența tipului pentru ambele expresii. Pentru `SumNeighborAgents`,
proprietatea trebuie să aparțină chiar tipului selectat, inclusiv dacă expresia se
află într-o regulă nefolosită sau într-o cerere cu zero pași. La evaluare sunt verificate
proprietățile agenților efectivi și rezultatul finit al sumei. Probabilitățile calculate
din număr sau sumă trebuie să fie în continuare în intervalul `[0, 1]`; de exemplu,
un număr de doi agenți folosit direct ca probabilitate produce eroare.

## Modificarea celulei de către agent

`SetCellProperty` atribuie o valoare unei proprietăți a celulei din poziția inițială
a agentului. `SetCellType` schimbă tipul acelei celule și pornește de la valorile
implicite ale noului tip. Ambele sunt permise numai în reguli cu `target: "agent"`;
regulile proprii ale celulelor folosesc în continuare `SetProperty` și `SetType`.

În JSON:

```json
{ "kind": "set_cell_property", "property": "food", "value": { "kind": "constant", "args": 0 } }
```

```json
{ "kind": "set_cell_type", "type_id": 2 }
```

Exemplu de consum, presupunând că tipul 0 declară `food`, iar tipul 1 declară `energy`:

```json
{
  "target": "agent",
  "condition": {
    "kind": "all",
    "args": [
      { "kind": "type_is", "args": 1 },
      { "kind": "cell_type_is", "args": 0 },
      {
        "kind": "compare",
        "args": {
          "left": { "kind": "cell_property", "args": "food" },
          "operator": { "kind": "greater" },
          "right": { "kind": "constant", "args": 0 }
        }
      }
    ]
  },
  "probability": 1,
  "actions": [
    {
      "kind": "set_property",
      "property": "energy",
      "value": {
        "kind": "add",
        "args": [
          { "kind": "property", "args": "energy" },
          { "kind": "cell_property", "args": "food" }
        ]
      }
    },
    {
      "kind": "set_cell_property",
      "property": "food",
      "value": { "kind": "constant", "args": 0 }
    }
  ]
}
```

La energie 10 și hrană 3, rezultatul este energie 13 și hrană 0. Cantitatea și
conversia în energie sunt stabilite de expresiile modelului. Motorul nu conține
un comportament special pentru hrană sau energie.

Toate expresiile folosesc stările de la începutul pasului. Mai multe atribuiri
ale aceleiași proprietăți în aceeași regulă păstrează ultima valoare calculată.
Un `SetCellType` șterge atribuirile anterioare pentru celulă și introduce valorile
implicite ale tipului nou; atribuiri ulterioare pot suprascrie aceste valori.
Tipul agentului și tipul celulei sunt urmărite separat la validare.

Modificările sunt întoarse în `ActionResult.cell_update: Option<CellUpdate>`.
`CellUpdate` se află în `src/models/action.rs` și conține:

- `type_id: Option<u8>`: tipul cerut explicit, dacă există o schimbare de tip.
- `properties: BTreeMap<String, f64>`: numai proprietățile atribuite, cu expresiile
  deja evaluate. O proprietate absentă din mapă nu este modificată.

`CellUpdate.apply()` construiește și validează starea rezultată. Modificarea este
validată întâi față de celula inițială, apoi la aplicare față de rezultatul regulii
proprii a celulei. De exemplu, dacă terenul devine piatră fără `food`, o atribuire
a lui `food` produce eroare. Un `SetCellType` explicit stabilește tipul folosit
pentru propriile atribuiri.

Simulatorul colectează toate cererile, verifică conflictele și apoi le aplică:

- Este permis cel mult un agent care modifică o anumită celulă într-un pas. Acesta
  poate atribui mai multe proprietăți sau poate schimba tipul în aceeași regulă.
- Doi agenți care modifică aceeași celulă produc o eroare de conflict, inclusiv
  pentru valori identice sau proprietăți diferite. Aceasta este politica simplă
  actuală pentru concurența dintre agenți. O atribuire care păstrează valoarea
  existentă contează tot ca o cerere de modificare.
- O regulă care nu se potrivește sau a cărei verificare probabilistică eșuează nu
  produce o cerere și nu participă la conflict.
- Regula proprie a celulei se aplică înaintea modificării agentului. Atribuirile
  agentului au prioritate pentru proprietățile cerute; celelalte proprietăți
  păstrează rezultatul regulii celulei. `SetCellType` înlocuiește tipul și proprietățile
  cu starea implicită cerută, apoi aplică propriile atribuiri.
- Ținta rămâne poziția inițială a agentului, chiar dacă acesta se deplasează.
  O deplasare blocată păstrează modificarea celulei și celelalte efecte ale regulii.

Conflictele și rezultatele invalide produc HTTP 422, cu generația, celula și agenții
implicați. Lumea inițială nu este modificată și nu se întoarce un istoric parțial.
Fiecare regulă trebuie să producă rezultate valide; valorile nu sunt ajustate automat.

## Probabilitatea calculată

În Rust, câmpul `Rule.probability` este acum un `Expression`. O probabilitate fixă
se scrie `Expression::Constant(0.2)`, iar una calculată folosește aceiași operatori
ca proprietățile și condițiile.

JSON-ul acceptă atât forma numerică existentă, cât și o expresie:

```json
"probability": 0.2
```

```json
"probability": {
  "kind": "divide",
  "args": [
    { "kind": "property", "args": "energy" },
    { "kind": "constant", "args": 100 }
  ]
}
```

Prima formă este transformată în `Expression::Constant` la citire. Când o definiție
Rust este serializată, probabilitatea folosește formatul de expresie, inclusiv pentru
constante: `{ "kind": "constant", "args": 0.2 }`.

`Power(base, exponent)` ridică baza la putere. În JSON se scrie
`{ "kind": "power", "args": [expresia_bazei, expresia_exponentului] }`.
Rezultatul trebuie să fie finit; rezultate precum rădăcina pătrată a unui număr
negativ sau zero la o putere negativă produc o eroare.

Pentru `n` vecini care transmit independent cu probabilitatea `p`, probabilitatea
cel puțin unei transmiteri este `1 - (1 - p)^n`. La `p = 0.2`, valorile pentru
0, 1, 2 și 3 vecini sunt 0, 0.2, 0.36 și 0.488. Aceasta este o formulă a modelului,
construită din expresii obișnuite; simulatorul nu conține o regulă dedicată infecțiilor.

La fiecare pas și pentru fiecare obiect:

1. Se verifică ținta și condiția regulii.
2. Pentru prima regulă potrivită se calculează probabilitatea din starea inițială și vecini.
3. Se verifică dacă valoarea este finită și în intervalul `[0, 1]`.
4. Generatorul aleator decide dacă sunt aplicate acțiunile.

Un rezultat invalid produce eroare; nu este ajustat automat. Dacă verificarea
probabilistică eșuează, obiectul rămâne neschimbat pentru acel pas și nu se încearcă
regulile următoare. Probabilitatea se recalculează la fiecare generație; seed-ul și
politica de mișcare se folosesc în continuare la fel.

## Validarea regulilor

Înaintea rulării sunt verificate toate condițiile și acțiunile, inclusiv ramurile
care ar fi sărite la execuție, regulile cu probabilitate zero și cererile cu zero pași.
Referințele la tipuri trebuie să existe în `object_types`, iar numele proprietăților
trebuie să fie declarate de cel puțin un tip. După `SetType`, proprietatea modificată
trebuie să aparțină tipului selectat de cea mai recentă astfel de acțiune.

Constantele și calculele alcătuite numai din constante trebuie să fie finite.
Împărțirea la un numitor constant zero, inclusiv unul calculat precum `2 - 2`, este
respinsă. Toleranța comparației aproximative trebuie să fie finită și nenegativă.
Erorile identifică regula și condiția sau acțiunea implicată.

Aceleași verificări se aplică expresiei probabilității. Dacă aceasta este complet
constantă, intervalul `[0, 1]` se verifică înainte de rulare, inclusiv la zero pași.
Pentru o valoare dependentă de lume, intervalul se verifică la evaluarea regulii.

La execuție rămân verificate proprietățile obiectului și ale vecinilor efectivi,
numitorii calculați din starea lumii și limitele stării rezultate. Expresiile din
acțiuni citesc în continuare starea inițială, inclusiv după o schimbare de tip.

## Deplasarea agenților

Deplasările sunt permise numai în reguli pentru agenți. O regulă poate conține cel
mult o acțiune de deplasare dintre `Move`, `MoveToFreeNeighbor`, `MoveToFreeCell`
și `MoveToRemovedNeighbor`.

```json
{
  "kind": "move",
  "row_offset": { "kind": "constant", "args": 0 },
  "col_offset": { "kind": "property", "args": "step" }
}
```

Offset-urile sunt calculate din starea de la începutul pasului și trebuie să fie
numere întregi din intervalul `i32`. Un offset pozitiv crește rândul sau coloana;
unul negativ le scade. Deplasarea poate avea un offset diferit de vecinătatea de citire.
`Wrap` trece peste margine pe partea opusă; `Fixed` anulează deplasarea în afara grilei.

Simulatorul calculează toate intențiile înainte de a schimba pozițiile:

- Dacă mai mulți agenți cer aceeași destinație, toate acele deplasări sunt anulate.
- Un agent poate intra în poziția altuia dacă acesta o eliberează. Schimburile de
  poziție, ciclurile și lanțurile către un loc liber sunt permise.
- Dacă un agent rămâne pe loc, sunt blocate și deplasările care depind de eliberarea
  poziției sale. Blocarea se propagă până când nu mai sunt alte schimbări.
- Suprapunerile existente inițial rămân acceptate. O poziție comună este eliberată
  numai dacă pleacă toți agenții aflați acolo; deplasările nu creează suprapuneri noi.

Blocarea unei deplasări păstrează celelalte schimbări de stare din regula aleasă,
de exemplu consumul de energie. Celulele terenului și identitatea agenților sunt păstrate.
Politica de conflict este deterministă; agenții care cer aceeași poziție nu au prioritate
în funcție de ordinea din listă. Alegerile probabilistice folosesc în continuare ordinea
agenților și generatorul inițializat cu seed-ul cererii.

`apply_actions()` și `apply_rules()` întorc un `ActionResult`: `state` conține
starea nouă, `movement` conține eventuala intenție de mișcare, `remove_agent` arată dacă agentul
trebuie eliminat, `neighbor_to_remove` conține ID-ul agentului vecin ales pentru
eliminare, `spawn` conține eventuala cerere de creare a unui agent, iar
`cell_update` conține modificarea cerută pentru celula agentului.
`Movement::Offset` conține offset-ul explicit sau ales din vecinătate,
`Movement::FreeCell` cere alegerea unei celule libere din întreaga grilă, iar
`Movement::RemovedNeighbor` cere intrarea în poziția țintei capturate.
`neighbor_updates` conține contribuțiile numerice ale unei reguli de celulă.
Acest rezultat este temporar; nu este un câmp
salvat în `ObjectState`, `Agent` sau în JSON-ul lumii.

### Alegerea unui vecin liber

`MoveToFreeNeighbor` alege o destinație din vecinătatea configurată, fără a calcula
manual offset-ul în fiecare regulă:

```json
{
  "kind": "move_to_free_neighbor",
  "selection": "random",
  "cell_type": 0
}
```

- `selection: "first"`: alege prima poziție eligibilă, în ordinea offset-urilor.
- `selection: "random"`: alege uniform între pozițiile eligibile distincte.
- `cell_type` este opțional. Dacă este prezent, celula destinației trebuie să aibă
  acel tip; dacă lipsește sau este `null`, sunt acceptate toate tipurile de teren.
- `offsets` este opțional. Dacă lipsește sau este `null`, se folosește vecinătatea
  modelului. O listă explicită restrânge direcțiile și stabilește ordinea lor pentru
  `first`. Poate avea cel mult 32 de offset-uri distincte, toate declarate în
  vecinătatea modelului. O listă goală păstrează agentul pe loc.

De exemplu, nisipul poate încerca în ordine jos, jos-stânga și jos-dreapta,
iar apa poate folosi alte direcții în aceeași definiție de automat:

```json
{
  "kind": "move_to_free_neighbor",
  "selection": "first",
  "cell_type": 0,
  "offsets": [[1, 0], [1, -1], [1, 1]]
}
```

O poziție eligibilă se află în grilă, este diferită de poziția curentă și nu are
niciun agent la începutul pasului. Marginea `Fixed` nu creează destinații în afara
grilei. `Wrap` înfășoară coordonatele, dar offset-urile care ajung la aceeași poziție
sunt numărate o singură dată, folosind primul offset eligibil din lista aleasă.
Astfel, o destinație nu primește șanse suplimentare doar fiindcă are mai multe alias-uri.

O poziție ocupată de un agent care va pleca sau va fi eliminat în acest pas devine
eligibilă pentru această alegere începând cu generația următoare. Filtrul de teren
citește tot starea de la începutul pasului. El verifică celula destinației, fără
verificări asupra traseului dintre origine și destinație.

După alegere, `ActionResult.movement` conține `Some(Movement::Offset(offset))`. Simulatorul aplică
aceeași rezolvare a conflictelor ca pentru `Move`: două cereri spre aceeași poziție
se anulează. Nu se face o nouă alegere după un conflict. Dacă nu există niciun vecin
eligibil, agentul rămâne pe loc; celelalte acțiuni din regulă se păstrează și nu se
încearcă o regulă ulterioară.

`NeighborSelection` este definit în `src/models/action.rs`, alături de acțiune.
`apply_actions()` primește acum generatorul aleatoriu de la `apply_rules()`.
Probabilitățile și deplasările aleatorii folosesc același RNG, inițializat o singură
dată în `run()`. Pentru aceeași configurație, aceeași ordine a agenților și același
seed, istoricul este reproductibil și lumea finală este identică în ambele moduri
`save_history`. Înlocuirea alegerii `first` cu `random` poate modifica și alegerile
probabilistice ulterioare, deoarece acestea folosesc aceeași secvență aleatoare.

Ținta trebuie să fie `agent`. Filtrul trebuie să indice un tip declarat, inclusiv
pentru zero pași sau reguli neexecutate. Combinarea cu orice altă acțiune de
deplasare în aceeași regulă este respinsă chiar dacă nu există destinații.
O vecinătate goală este validă și produce o deplasare fără destinație.

### Alegerea unui loc liber din întreaga grilă

`MoveToFreeCell` permite relocarea globală, folosită de Schelling:

```json
{ "kind": "move_to_free_cell", "selection": "random", "cell_type": 0 }
```

`first` parcurge rândurile, apoi coloanele. `random` alege uniform între celulele
eligibile. `cell_type` este opțional și are același sens ca la deplasarea locală.
Selecția citește terenul și ocuparea de la începutul pasului, folosind RNG-ul comun
imediat după evaluarea regulii agentului. Destinațiile sunt rezolvate ulterior
împreună cu celelalte deplasări. Cererile către aceeași poziție se anulează, fără
o nouă alegere. Fără destinații, agentul rămâne pe loc și păstrează celelalte
efecte ale regulii. `apply_actions()` întoarce `Some(Movement::FreeCell { ... })`;
simulatorul are acces la întreaga lume și face selecția efectivă.

## Eliminarea agenților

`RemoveAgent` elimină agentul care execută regula. Este permisă numai pentru ținta
`agent` și trebuie să fie singura acțiune din regula respectivă. Combinarea cu orice
altă acțiune este respinsă înaintea rulării, inclusiv
pentru zero pași, condiții false și probabilitate zero. În JSON se scrie
`{ "kind": "remove_agent" }`, fără alte câmpuri.

În Rust se construiește cu `Action::RemoveAgent {}`. Acoladele goale permit aplicarea
verificării `deny_unknown_fields` și pentru această variantă fără parametri.

Exemplu de regulă pentru tipul 1, presupunând că acesta declară proprietatea `energy`:

```json
{
  "target": "agent",
  "condition": {
    "kind": "all",
    "args": [
      { "kind": "type_is", "args": 1 },
      {
        "kind": "compare",
        "args": {
          "left": { "kind": "property", "args": "energy" },
          "operator": { "kind": "less_or_equal" },
          "right": { "kind": "constant", "args": 0 }
        }
      }
    ]
  },
  "probability": 1,
  "actions": [{ "kind": "remove_agent" }]
}
```

Regula de eliminare trebuie pusă înaintea regulilor generale pentru același tip,
fiindcă motorul selectează prima regulă potrivită. `energy` și pragul zero aparțin
definiției modelului; simulatorul nu le verifică automat. Probabilitatea eliminării
poate fi și o expresie. Dacă verificarea probabilistică eșuează, agentul rămâne
neschimbat și nu sunt încercate regulile următoare.

Toate deciziile citesc lumea de la începutul pasului. Dacă un agent are energie 1,
iar o altă regulă o reduce la 0, regula de mai sus îl elimină în pasul următor.
Nu sunt aplicate două reguli aceluiași obiect în aceeași generație.

Simulatorul păstrează numai agenții care supraviețuiesc, apoi rezolvă deplasările lor.
Poziția unui agent eliminat devine disponibilă în același pas. Un alt ocupant care
rămâne acolo continuă să blocheze intrarea, iar mai mulți agenți care cer aceeași
poziție eliberată sunt supuși regulii obișnuite de conflict. Identitatea și ordinea
relativă a agenților rămași se păstrează; celula terenului nu este eliminată.

Generațiile anterioare din istoric păstrează agentul. Zero pași nu aplică eliminarea,
iar simularea poate continua și după ce lista de agenți devine goală.

## Eliminarea unui agent vecin

`RemoveNeighborAgent` selectează un agent observat în vecinătate după tipul său:

```json
{
  "kind": "remove_neighbor_agent",
  "type_id": 1,
  "selection": "random"
}
```

`type_id` și `selection` sunt obligatorii. `first` alege primul agent potrivit din
ordinea listei `world.agents`, iar `random` alege uniform între indivizii potriviți,
folosind RNG-ul simulării. Agenții de alte tipuri sunt ignorați. Mai mulți agenți
din aceeași poziție sunt indivizi separați; offset-urile care ajung la aceeași poziție
nu dublează indivizii observați. Poziția proprie, inclusiv alți agenți aflați acolo,
este exclusă conform politicii vecinătății. Agenții nou-creați devin vizibili la
generația următoare.

Acțiunea este permisă numai pentru ținta `agent`, cel mult o dată într-o regulă.
Poate fi combinată cu modificări proprii, deplasare, scrierea terenului sau crearea
unui copil. `RemoveAgent`, eliminarea proprie, rămâne o acțiune folosită singură.

O regulă poate acorda energie la captură folosind acțiunile existente:

```json
[
  { "kind": "remove_neighbor_agent", "type_id": 1, "selection": "first" },
  {
    "kind": "set_property", "property": "energy",
    "value": { "kind": "add", "args": [
      { "kind": "property", "args": "energy" },
      { "kind": "constant", "args": 10 }
    ] }
  }
]
```

Selecția țintei se face înaintea celorlalte acțiuni, indiferent de poziția acestei
acțiuni în listă. Dacă nu există un agent potrivit, întreaga regulă nu produce efecte;
nu se încearcă o regulă ulterioară. Tipurile și referințele rămân validate înainte
de execuție, inclusiv pentru reguli neexecutate și zero generații.

Simulatorul colectează toate intențiile din lumea veche și aplică următoarea politică:

- O singură cerere spre o țintă care nu se elimină singură acceptă captura.
- Două sau mai multe cereri spre aceeași țintă sunt toate anulate. Ținta nu este
  eliminată de aceste cereri, iar atacatorii păstrează starea inițială, fără energie,
  deplasare, naștere sau modificare de teren din regula anulată. Nu aleg altă țintă.
- Dacă ținta execută `RemoveAgent`, captura este anulată fără recompensă.
- Atacurile acceptate sunt simultane: două atacuri reciproce elimină ambii agenți;
  într-un lanț A atacă B și B atacă C, sunt eliminați B și C.
- Agenții eliminați nu transmit celelalte efecte ale regulilor lor: deplasări,
  nașteri sau scrieri ale terenului. Regulile autonome ale celulelor se păstrează.
- O captură acceptată eliberează poziția pentru deplasări explicite și nașteri în
  același pas. Alegerea `MoveToFreeNeighbor` folosește tot ocuparea de la începutul
  pasului, deci vede acea poziție liberă începând cu generația următoare.

Erorile din regulile evaluate sunt raportate înainte de rezolvarea conflictelor;
o eroare de expresie sau de stare nu este ascunsă de o anulare ulterioară. După o
captură acceptată, conflictele de deplasare sau naștere păstrează politica lor
existentă și nu anulează captura ori recompensa.

`ActionResult.neighbor_to_remove` este `Option<u64>` și reprezintă o intenție,
fără a modifica direct lumea. `ActionResult::unchanged()` construiește un rezultat
fără efecte, folosit când nu există țintă sau captura este anulată.
`src/simulator.rs` rezolvă cererile înainte să aplice efectele supraviețuitorilor.

### Deplasarea în poziția țintei capturate

`MoveToRemovedNeighbor` se scrie `{ "kind": "move_to_removed_neighbor" }` și cere
în aceeași regulă o acțiune `RemoveNeighborAgent`. Este o acțiune de deplasare și
nu poate fi combinată cu altă deplasare. Ordinea ei în listă nu schimbă ținta.
După acceptarea capturii, simulatorul folosește poziția veche a individului
selectat; funcționează și peste marginea circulară. Un atac anulat anulează și
această deplasare. Un atacator eliminat nu se mai deplasează. Conflictele obișnuite
pot bloca intrarea, de exemplu dacă mai rămâne un ocupant în poziția țintei;
captura acceptată și recompensa se păstrează conform politicii existente.

## Contribuții numerice către celule vecine

`AddNeighborProperty` permite unei reguli de celulă să trimită o contribuție
numerică spre o poziție vecină. Offset-ul trebuie declarat în vecinătate:

```json
{
  "kind": "add_neighbor_property",
  "offset": [-1, 0],
  "property": "grains",
  "value": { "kind": "constant", "args": 1 }
}
```

Expresia `value` citește starea veche a celulei sursă și vecinii ei. Valorile
pozitive adună, cele negative scad. Acțiunea nu scade automat valoarea din sursă;
regula configurează separat această schimbare prin `set_property`. Sandpile scade
patru granule din sursă și trimite patru contribuții de câte o granulă.

Într-un pas, ordinea este: stările autonome ale celulelor; deciziile agenților și
rezolvarea capturilor; scrierile de teren ale agenților supraviețuitori; însumarea
și aplicarea contribuțiilor; deplasările; apoi nașterile. Toate deciziile citesc
lumea veche. Contribuțiile primite nu declanșează o nouă regulă în același pas.

Contribuțiile sunt însumate pentru fiecare poziție și proprietate, apoi adăugate
valorii rezultate după regulile autonome și scrierile agenților. Mai multe surse
pot contribui la aceeași proprietate. `min_value`, `max_value`, `integer_only` și
caracterul finit sunt verificate pe rezultatul agregat. Suma intermediară a
contribuțiilor trebuie de asemenea să rămână finită. Stările proprii și scrierile
agenților sunt validate separat înaintea contribuțiilor; un rezultat propriu
invalid nu este reparat printr-o contribuție ulterioară.

La `wrap`, fiecare offset produce o contribuție: două direcții care ajung în
aceeași poziție se însumează, inclusiv când ajung înapoi la sursă. La `fixed`,
contribuțiile spre exterior sunt absorbite; starea fixă nu este modificată.
Pentru destinațiile interioare, proprietatea trebuie să existe în tipul final al
celulei. Dacă tipul s-a schimbat și proprietatea a dispărut, simularea raportează
eroare. Un pas cu eroare nu întoarce rezultate parțiale și nu modifică lumea primită.

Acțiunea este permisă numai în reguli de celule; scrierile agenților folosesc în
continuare `set_cell_property` și `set_cell_type`. `NeighborPropertyUpdate` și
`Movement` sunt tipuri temporare în `action.rs`, folosite în rezultatul evaluării;
nu adaugă câmpuri lumii salvate.

## Crearea agenților

`SpawnAgent` poate fi folosită într-o regulă pentru o celulă sau pentru un agent,
cel mult o dată în aceeași regulă. Poate fi combinată cu schimbări de stare și, în
regulile agenților, cu `Move`. `RemoveAgent` rămâne o acțiune care trebuie folosită singură.

Acțiunea specifică tipul noului agent, offset-urile față de poziția inițială a
obiectului care execută regula și proprietățile pe care le suprascrie. Toate
proprietățile nespecificate primesc valorile implicite ale tipului nou. În JSON,
câmpul `properties` poate lipsi sau poate fi `{}`.

Exemplu de reproducere, presupunând că tipul 1 declară `energy`:

```json
{
  "target": "agent",
  "condition": {
    "kind": "all",
    "args": [
      { "kind": "type_is", "args": 1 },
      {
        "kind": "compare",
        "args": {
          "left": { "kind": "property", "args": "energy" },
          "operator": { "kind": "greater_or_equal" },
          "right": { "kind": "constant", "args": 20 }
        }
      }
    ]
  },
  "probability": 1,
  "actions": [
    {
      "kind": "set_property",
      "property": "energy",
      "value": {
        "kind": "divide",
        "args": [
          { "kind": "property", "args": "energy" },
          { "kind": "constant", "args": 2 }
        ]
      }
    },
    {
      "kind": "spawn_agent",
      "type_id": 1,
      "row_offset": { "kind": "constant", "args": 0 },
      "col_offset": { "kind": "constant", "args": 1 },
      "properties": {
        "energy": {
          "kind": "divide",
          "args": [
            { "kind": "property", "args": "energy" },
            { "kind": "constant", "args": 2 }
          ]
        }
      }
    }
  ]
}
```

La energie inițială 20, părintele rămâne cu 10, iar copilul primește 10. Expresiile
copilului și offset-urile citesc starea de la începutul pasului, inclusiv dacă o
acțiune anterioară a modificat proprietățile sau tipul părintelui. O deplasare a
părintelui nu schimbă originea offset-urilor copilului.

În Rust, `ActionResult.spawn` este `Option<AgentSpawn>`. `AgentSpawn` conține
`state` și `offset`; simulatorul stabilește poziția finală și ID-ul. Acest rezultat
temporar se află în `src/models/action.rs`, alături de acțiunile care îl produc.

Ordinea unui pas este: evaluarea regulilor din lumea inițială, rezolvarea cererilor
de eliminare a vecinilor, eliminarea agenților,
verificarea și aplicarea modificărilor celulelor cerute de agenți, rezolvarea
deplasărilor supraviețuitorilor și apoi rezolvarea nașterilor:

- Un copil poate ocupa o poziție eliberată prin eliminare sau deplasare în același pas.
- O poziție ocupată după deplasări anulează nașterea. Deplasările au astfel prioritate.
- Dacă mai multe cereri de creare ajung în aceeași poziție, toate sunt anulate,
  inclusiv cererile provenite de la celule și de la agenți.
- `Wrap` aplică înfășurarea offset-urilor; `Fixed` anulează nașterile în afara grilei.
- Agenții nou-creați sunt adăugați după supraviețuitori și execută prima regulă
  abia la generația următoare. Celula terenului continuă să existe în poziția lor.

Modificările părintelui se păstrează și când nașterea este anulată din cauza poziției.
În exemplul de mai sus, energia este redusă și dacă poziția copilului este ocupată.
Aceasta este politica actuală, la fel cum o deplasare blocată păstrează schimbările
de stare ale agentului. O stare invalidă a copilului produce eroare înainte de
rezolvarea pozițiilor; nu este ascunsă de un conflict sau de o margine fixă.

ID-urile copiilor sunt `u64`, cresc peste cel mai mare ID din lumea inițială și sunt
alocate numai nașterilor acceptate. Dacă inițial nu există agenți, primul ID este 1.
`run()` păstrează contorul între generații și nu reutilizează ID-uri după eliminare.
Cereri distincte HTTP sunt rulări independente. La epuizarea intervalului `u64`, o
naștere acceptată produce eroare; o naștere anulată nu consumă un ID.

Ordinea alocării este cea a cererilor acceptate: celulele pe rânduri și coloane,
apoi agenții în ordinea listei inițiale a pasului. Aceasta face rezultatele și
ID-urile reproductibile pentru aceeași cerere și același seed. `step()` folosit
independent pornește de la ID-urile lumii primite; pentru un istoric cu ID-uri
nereutilizate se folosește `run()`, care păstrează contorul pe durata rulării.

## Erori și limite

Erorile endpoint-ului de simulare au forma `{ "error": "mesaj" }`:

- HTTP 400: JSON cu sintaxă incorectă sau parametri numerici invalizi în rutele de citire.
- HTTP 404: simularea nu există sau generația cerută nu a fost păstrată.
- HTTP 415: lipsește `Content-Type: application/json`.
- HTTP 422: structură JSON, model, stare sau parametri invalizi; pentru erori de
  execuție, mesajul identifică generația și celula/agentul implicat.
- HTTP 413: corpul cererii depășește 256 KiB.
- HTTP 500: worker-ul nu a putut termina calculul sau operația de salvare/citire a eșuat.
  Detaliile erorilor de stocare sunt scrise în consola serverului; răspunsul HTTP
  conține un mesaj general.

Calculul unui lot prin `/run` permite maximum 1.000 de pași. Calculul prin `/step`
nu are acest plafon cumulativ. Definiția permite 64 de reguli, 32 de offset-uri și
512 noduri în total pentru condiții, acțiuni și expresii (inclusiv probabilitățile),
cu adâncime maximum 16.
Numele proprietăților au maximum 64 de octeți.

Istoricul unui lot `/run` poate avea maximum 100.000 de unități. Cu `save_history: true`, înaintea
rulării se verifică estimarea:

```text
(număr celule + număr agenți) × (pași + 1) × (1 + maximum proprietăți pe un tip)
```

La execuție se adună unitățile fiecărei generații efective:

```text
sumă pentru toate generațiile [(număr celule + număr agenți) × (1 + maximum proprietăți pe un tip)]
```

Această a doua verificare include agenții creați și produce HTTP 422 la generația
care depășește limita, fără un istoric parțial în răspuns. Estimarea inițială singură
nu ar putea limita creșterea populației.

Cu `save_history: false`, estimarea inițială folosește o singură generație:

```text
(număr celule + număr agenți) × (1 + maximum proprietăți pe un tip)
```

La execuție, aceeași limită se aplică separat fiecărei lumi calculate, inclusiv
generațiilor intermediare și agenților nou-creați. Unitățile generațiilor eliminate
nu se adună. Limita de 1.000 de pași se aplică numai loturilor `/run`.
La `/step`, bugetul verifică doar noua lume, iar generațiile precedente rămân în SQLite.

Limita include și tipurile în care obiectele se pot transforma. Limitele
sunt definite în `src/models/simulation.rs`; dimensiunea cererii este în `src/api.rs`.

Verificare: `cargo test --locked` și `cargo fmt --check`.
Testele HTTP folosesc baze SQLite separate în memorie. Testul de persistență după
redeschidere folosește un fișier din directorul temporar al sistemului, șters la final;
testele nu folosesc baza configurată pentru server.
