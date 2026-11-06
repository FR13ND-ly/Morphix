import { HttpClient, HttpErrorResponse, HttpParams } from '@angular/common/http';
import { inject, Injectable } from '@angular/core';
import { defer, map } from 'rxjs';
import {
  SimulationGeneration,
  SimulationListQuery,
  SimulationListResponse,
  SimulationRequest,
  SimulationResponse,
  SimulationValidationResponse,
  StoredSimulation,
  World,
} from './simulation.models';

@Injectable({
  providedIn: 'root',
})
export class SimulationsApi {
  private readonly http = inject(HttpClient);
  private readonly url = '/api/simulations';

  list(query: Partial<SimulationListQuery> = {}) {
    const params = new HttpParams({
      fromObject: {
        page: query.page ?? 1,
        page_size: query.page_size ?? 20,
        search: query.search ?? '',
        sort: query.sort ?? 'created_at',
        order: query.order ?? 'desc',
      },
    });
    return this.http.get<SimulationListResponse>(this.url, { params });
  }

  get(id: string | number) {
    return this.http.get<StoredSimulation>(`${this.url}/${encodeURIComponent(id)}`).pipe(
      map((simulation) => {
        checkRequestIntegers(simulation.request);
        checkHistoryIntegers(simulation.history);
        return simulation;
      }),
    );
  }

  getExample(name: string) {
    return this.http
      .get<SimulationRequest>(`${this.url}/examples/${encodeURIComponent(name)}`)
      .pipe(
        map((request) => {
          checkRequestIntegers(request);
          return request;
        }),
      );
  }

  getGeneration(id: string | number, generation: number) {
    return this.http
      .get<SimulationGeneration>(`${this.url}/${encodeURIComponent(id)}/generations/${generation}`)
      .pipe(
        map((snapshot) => {
          checkWorldIntegers(snapshot.world);
          return snapshot;
        }),
      );
  }

  step(id: number, generation: number) {
    return this.http.post<SimulationGeneration>(`${this.url}/${id}/step`, { generation }).pipe(
      map((snapshot) => {
        checkWorldIntegers(snapshot.world);
        return snapshot;
      }),
    );
  }

  delete(id: number) {
    return this.http.delete<void>(`${this.url}/${id}`);
  }

  validate(request: SimulationRequest) {
    return defer(() => {
      checkRequestIntegers(request);
      return this.http.post<SimulationValidationResponse>(`${this.url}/validate`, request);
    });
  }

  run(request: SimulationRequest) {
    return defer(() => {
      checkRequestIntegers(request);
      return this.http.post<SimulationResponse>(`${this.url}/run`, request);
    }).pipe(
      map((response) => {
        checkHistoryIntegers(response.history);
        return response;
      }),
    );
  }
}

export function simulationErrorMessage(error: unknown): string {
  if (error instanceof HttpErrorResponse) {
    if (error.status === 0) {
      return 'Backendul nu poate fi contactat. Verifică dacă serverul Rust este pornit.';
    }
    if (typeof error.error?.error === 'string') {
      return error.error.error;
    }
    return `Cererea a eșuat (HTTP ${error.status}).`;
  }
  return error instanceof Error ? error.message : 'A apărut o eroare neașteptată.';
}

// Rust accepts u64; JavaScript numbers must stay within its exact integer range.
function checkRequestIntegers(request: SimulationRequest) {
  checkInteger(request?.seed);
  checkWorldIntegers(request?.initial_world);
}

function checkWorldIntegers(world: World | undefined) {
  if (Array.isArray(world?.agents)) {
    for (const agent of world.agents) {
      checkInteger(agent?.id);
    }
  }
}

function checkHistoryIntegers(history: SimulationGeneration[]) {
  for (const snapshot of history) {
    checkWorldIntegers(snapshot.world);
  }
}

function checkInteger(value: unknown) {
  if (typeof value === 'number' && Number.isInteger(value) && !Number.isSafeInteger(value)) {
    throw new Error('Frontendul acceptă seed-uri și ID-uri de agenți până la 9007199254740991.');
  }
}
