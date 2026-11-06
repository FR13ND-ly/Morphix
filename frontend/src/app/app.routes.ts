import { Routes } from '@angular/router';

export const routes: Routes = [
  { path: '', pathMatch: 'full', redirectTo: 'simulations' },
  {
    path: 'simulations',
    title: 'Simulări — morphix',
    loadComponent: () =>
      import('./simulations/simulation-list/simulation-list').then((m) => m.SimulationList),
  },
  {
    path: 'simulations/new',
    title: 'Simulare nouă — morphix',
    loadComponent: () =>
      import('./simulations/simulation-create/simulation-create').then((m) => m.SimulationCreate),
  },
  {
    path: 'simulations/:id',
    title: 'Rezultatul simulării — morphix',
    loadComponent: () =>
      import('./simulations/simulation-details/simulation-details').then(
        (m) => m.SimulationDetails,
      ),
  },
  { path: '**', redirectTo: 'simulations' },
];
