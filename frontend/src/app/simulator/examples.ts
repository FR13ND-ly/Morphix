export const EXAMPLES = [
  {
    file: 'game-of-life',
    label: 'Game of Life',
    category: 'Celule',
    description:
      'Nașterea și supraviețuirea depind de vecini. Urmărește oscilatorul care își schimbă orientarea la fiecare pas.',
  },
  {
    file: 'forest-fire',
    label: 'Incendiu de pădure',
    category: 'Probabilistic',
    description:
      'Focul se propagă între copaci. Pădurea se regenerează, iar aprinderile spontane sunt probabilistice.',
  },
  {
    file: 'diffusion',
    label: 'Difuzia căldurii',
    category: 'Numeric',
    description:
      'Temperatura se apropie de media vecinilor. Valorile și intensitatea culorii arată cum se distribuie căldura.',
  },
  {
    file: 'artificial-ecosystem',
    label: 'Ecosistem artificial',
    category: 'Agenți',
    description:
      'Iarba crește, iepurii se hrănesc, iar vulpile vânează. Energia controlează reproducerea și supraviețuirea.',
  },
  {
    file: 'rule-30',
    label: 'Rule 30',
    category: 'Celule',
    description:
      'Un automat pe un singur rând. Celulele citesc vecinul stâng, starea proprie și vecinul drept.',
  },
  {
    file: 'brians-brain',
    label: 'Brian’s Brain',
    category: 'Celule',
    description:
      'Trei stări: inactivă, activă și în repaus. Doi vecini activi pot declanșa o activare nouă.',
  },
  {
    file: 'cyclic',
    label: 'Automat ciclic',
    category: 'Celule',
    description: 'Celulele trec prin patru faze când suficienți vecini se află în faza următoare.',
  },
  {
    file: 'langtons-ant',
    label: 'Furnica lui Langton',
    category: 'Agenți',
    description:
      'Furnica se rotește după culoarea terenului, schimbă celula și înaintează pe grila circulară.',
  },
  {
    file: 'sandpile',
    label: 'Sandpile',
    category: 'Numeric',
    description:
      'O celulă cu cel puțin patru granule trimite câte una spre fiecare vecin cardinal. Transferurile sunt simultane.',
  },
  {
    file: 'traffic',
    label: 'Trafic circular',
    category: 'Agenți',
    description:
      'Mașinile avansează cu o celulă când locul din față era liber la începutul pasului.',
  },
  {
    file: 'predator-prey',
    label: 'Prădător și pradă',
    category: 'Agenți',
    description: 'Prădătorul capturează o pradă vecină, primește energie și intră în poziția ei.',
  },
  {
    file: 'epidemic',
    label: 'Epidemie SIR',
    category: 'Probabilistic',
    description:
      'Vecinii infectați cresc probabilitatea transmiterii. După perioada de infecție, celulele se recuperează.',
  },
  {
    file: 'schelling',
    label: 'Modelul Schelling',
    category: 'Agenți',
    description: 'Agenții nemulțumiți de vecinătate se pot muta în orice loc liber al grilei.',
  },
  {
    file: 'voter',
    label: 'Opinia majorității',
    category: 'Celule',
    description: 'Celulele adoptă opinia majoritară. La egalitate, opinia rămâne aceeași.',
  },
  {
    file: 'crystal-growth',
    label: 'Creșterea cristalelor',
    category: 'Celule',
    description:
      'Dintr-un cristal inițial apar celule noi acolo unde există exact un vecin cristal.',
  },
  {
    file: 'falling-sand',
    label: 'Nisip și apă',
    category: 'Agenți',
    description:
      'Nisipul cade în jos sau diagonal. Apa poate curge lateral, iar rocile blochează trecerea.',
  },
  {
    file: 'moving-agents',
    label: 'Conflicte de deplasare',
    category: 'Agenți',
    description: 'Doi agenți cer aceeași destinație. Ambele deplasări se anulează.',
  },
];
