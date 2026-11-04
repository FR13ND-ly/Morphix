import { Service, inject } from '@angular/core';
import { HttpClient } from '@angular/common/http';
import { firstValueFrom } from 'rxjs';
import { SimulationRequest, SimulationResponse } from './simulation.models';

@Service()
export class SimulationService {
  private readonly http = inject(HttpClient);

  loadExample(file: string) {
    return firstValueFrom(this.http.get<SimulationRequest>(`/api/simulations/examples/${file}`));
  }

  health() {
    return firstValueFrom(this.http.get('/api/health', { responseType: 'text' }));
  }

  validate(request: SimulationRequest) {
    return firstValueFrom(this.http.post<{ valid: boolean }>('/api/simulations/validate', request));
  }

  run(request: SimulationRequest) {
    return firstValueFrom(this.http.post<SimulationResponse>('/api/simulations/run', request));
  }
}
