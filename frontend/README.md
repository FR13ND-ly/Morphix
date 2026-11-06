# morphix — frontend

Aplicație Angular 22 cu Tailwind CSS. Interfața folosește o navigare simplă,
formulare și tabele, fără biblioteci suplimentare de componente.

## Pornire

Într-un terminal, din `backend`, pornește serverul cu `cargo run`.
În alt terminal, din `frontend`:

```sh
npm ci
npm start
```

Deschide `http://localhost:4200`. Cererile către `/api/**` sunt redirecționate
către `http://127.0.0.1:3000`, eliminând prefixul `/api`. Adresa backendului se
schimbă în `proxy.conf.json`. La publicare, serverul web trebuie să facă aceeași
redirecționare și să servească `index.html` pentru rutele aplicației.

## Pagini

- Un click pe **morphix** din header deschide lista simulărilor.
- `/simulations`: lista salvată în SQLite, căutare după nume, sortare, paginare și ștergere.
  Filtrele sunt păstrate în URL. Căutarea se aplică după o pauză scurtă la tastare.
- `/simulations/new`: alegerea unuia dintre cele patru exemple temporare, încărcarea lui,
  alegerea seed-ului și a dimensiunii grilei, validare statică și pornire
  de la generația 0. Configurațiile sunt primite de la backend.
- `/simulations/:id`: generațiile salvate, grila celulelor, inspector de
  proprietăți, marcajele și tabelul agenților. Simularea poate fi ștearsă și de aici.

Definițiile automatelor și lumile inițiale sunt în `backend/examples/`.
Formularul expune doar Game of Life, Incendiu de pădure, Difuzie și Ecosistem cu
agenți. Acestea acoperă celule discrete, reguli probabilistice, proprietăți numerice
și agenți mobili. Alegerea exemplelor este o etapă temporară pentru testarea
simulatorului. Direcția finală este crearea printr-un prompt: serviciul AI va
întoarce definiția automatului și lumea inițială în contractul folosit deja de simulator.
Această funcție nu este încă implementată; pagina explică etapa actuală.
Backendul expune aceleași patru exemple temporare.
Nu există editor de reguli sau import de fișiere în interfață. Parametrii sunt
inițial cei din exemplul încărcat sau din configurația salvată, apoi pot fi schimbați.
Butonul „Seed aleatoriu” generează un întreg fără semn pe 32 de biți. Pentru
exemplele noi, „Generează o lume aleatorie după seed” este activ implicit.
Generatorul comun folosește definițiile tipurilor, fără ramuri după numele automatului:

- Fiecare tip are o pondere pentru celule, între 0 și 100. Ponderile se normalizează:
  7 și 3 au același efect ca 70 și 30. Fiecare celulă este aleasă independent;
  numărul efectiv al celulelor de un tip poate varia între seed-uri.
- Fiecare proprietate are un interval inițial. Limitele egale fixează valoarea;
  altfel, se extrage uniform o valoare zecimală sau un întreg, după definiție.
  Intervalul trebuie să respecte limitele tipului; intervalele întregi trebuie
  să poată fi reprezentate exact în JavaScript.
- Fiecare tip are și un număr de agenți și, opțional, un tip de teren permis.
  Agenții primesc poziții distincte și ID-uri noi, consecutive. Dacă nu există
  suficiente celule permise, generarea este refuzată cu un mesaj explicativ.

Ponderile implicite sunt proporțiile observate în exemplu. Numărul agenților,
terenul lor și intervalele proprietăților sunt deduse tot din exemplu;
proprietățile tipurilor încă nefolosite pornesc de la valorile implicite declarate.
Toate tipurile declarate pot fi configurate, chiar dacă nu apar în lumea inițială.
Setările se găsesc în „Distribuție și proprietăți inițiale”.

Opțiunea aleatorie se poate dezactiva pentru lumea exactă din JSON.
Același seed și aceeași configurație reproduc aceeași inițializare; validarea și
pornirea construiesc rezultatul din sursa nemodificată. Seed-ul controlează și
regulile probabilistice din backend.
Nu se alege dinainte un număr de generații. „Înainte” afișează următoarea generație
salvată sau cere backendului să calculeze și să salveze una nouă. „Înapoi” consultă
istoricul existent. Numărul total al pașilor nu are un plafon fix.
La redeschiderea unei simulări, aceeași evoluție poate continua cu același seed.
Interfața pregătește până la trei generații înaintea celei afișate, prin cereri
secvențiale. La avansare folosește imediat o generație disponibilă și completează
rezerva în fundal. Pregătirea nu schimbă generația afișată și se oprește când
rezerva este completă sau când pagina se închide. Generațiile pregătite sunt
salvate de backend și apar în numărul generațiilor calculate.

Redimensionarea păstrează celulele existente în colțul stânga-sus. Celulele noi
folosesc tipul primei celule inițiale și valorile implicite ale proprietăților lui.
Generarea aleatorie folosește dimensiunea aleasă pentru întreaga lume.
Micșorarea decupează celulele din afara grilei, dar este refuzată dacă ar elimina
agenți. Alegerea se confirmă prin „Aplică dimensiunea” sau se anulează prin „Renunță”.
Rularea și validarea sunt dezactivate cât timp această alegere este deschisă.
Frontendul verifică bugetul de 100.000 de unități de stare/proprietate pentru o lume înainte
de a aloca grila și de a trimite cererea; backendul rămâne responsabil de validarea completă.

Grila folosește nuanțe pentru tipurile de obiecte și puncte albastre pentru
agenți. Legenda include numele și ID-urile tipurilor. Selectarea unei celule
afișează valorile proprietăților și agenții aflați în acea poziție.
Grilele mari pot fi derulate în interiorul zonei de afișare.

Cu `save_history: false` este disponibilă numai generația finală; interfața
folosește numărul generației din răspuns, nu poziția ei în vector.
Fiecare generație nouă se calculează în backend, prin `POST /simulations/{id}/step`.
Butonul de avansare este dezactivat numai când utilizatorul a cerut o generație
care încă se calculează; navigarea în istoricul disponibil rămâne activă.
Indicatorul de calcul ocupă un loc fix lângă controale și nu deplasează grila.
Erorile unei pregătiri opresc completarea automată, fără să schimbe grila; la
cererea explicită a următorului pas, calculul poate fi reîncercat.
Simulările vechi cu `save_history: false` continuă numai la cerere, deoarece
backendul lor păstrează doar ultima lume.
Validarea statică nu garantează că toate valorile produse în timpul rulării
vor fi valide.

„Șterge” deschide o confirmare în pagină; numai „Șterge definitiv” trimite
`DELETE /simulations/{id}`. Ștergerea elimină simularea, istoricul și checkpoint-ul
din SQLite. Din pagina detaliilor se revine la listă. Lista își păstrează filtrele
și revine pe pagina precedentă dacă ultima înregistrare a paginii curente dispare.
La eroare, datele rămân afișate și ștergerea poate fi reîncercată.
În timpul confirmării din detalii nu se mai lansează pregătiri de generații noi.

Frontendul acceptă seed-uri și ID-uri de agenți până la `9007199254740991`,
limita reprezentării exacte a întregilor JavaScript. Valorile mai mari sunt
refuzate explicit; backendul își păstrează suportul pentru `u64`.

## Organizare

- `src/styles.css`: stilurile comune Tailwind și culorile grilei.
- `src/app/simulations/simulation.models.ts`: contractele TypeScript pentru API.
- `src/app/simulations/simulations-api.ts`: cererile HTTP și mesajele de eroare.
- `simulation-list/`: lista și filtrele.
- `simulation-create/`: alegerea exemplului, validarea și crearea stării inițiale.
- `world-initializer.ts`: deducerea setărilor și generarea comună a celulelor,
  proprietăților și agenților. Întoarce un `World` obișnuit, validat apoi de backend.
- `simulation-details/`: consultarea istoricului și calculul pas cu pas.
- `app.routes.ts`: paginile încărcate la accesare.

Componentele și serviciile se generează cu Angular CLI, de exemplu
`npx ng g c simulations/simulation-create` și
`npx ng g s simulations/simulations-api --injectable`.

## Verificare

```sh
npm run build
npm test -- --watch=false
```

Testele folosesc răspunsuri HTTP controlate și nu necesită un backend pornit.
