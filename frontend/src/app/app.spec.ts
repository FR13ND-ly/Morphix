import { TestBed } from '@angular/core/testing';
import { provideHttpClient } from '@angular/common/http';
import { HttpTestingController, provideHttpClientTesting } from '@angular/common/http/testing';
import { NavigationEnd, provideRouter, Router } from '@angular/router';
import { filter, firstValueFrom, take } from 'rxjs';
import { App } from './app';
import { routes } from './app.routes';

describe('App', () => {
  beforeEach(async () => {
    await TestBed.configureTestingModule({
      imports: [App],
      providers: [provideRouter(routes), provideHttpClient(), provideHttpClientTesting()],
    }).compileComponents();
  });

  it('opens the list, creates from examples, and returns through the morphix logo', async () => {
    const fixture = TestBed.createComponent(App);
    const router = TestBed.inject(Router);
    const http = TestBed.inject(HttpTestingController);
    const flushList = () =>
      http
        .expectOne((req) => req.url === '/api/simulations')
        .flush({ items: [], total: 0, page: 1, page_size: 20 });
    fixture.detectChanges();
    await router.navigateByUrl('/');
    flushList();
    await fixture.whenStable();
    expect(fixture.nativeElement.querySelector('h1')?.textContent).toBe('Simulări');
    expect(fixture.nativeElement.querySelector('header').textContent.trim()).toBe('morphix');
    expect(fixture.nativeElement.querySelectorAll('header a').length).toBe(1);

    fixture.nativeElement.querySelector('main a[href="/simulations/new"]').click();
    await fixture.whenStable();
    expect(router.url).toBe('/simulations/new');
    expect(fixture.nativeElement.querySelector('select#example')).toBeTruthy();
    expect(fixture.nativeElement.querySelector('input[type="file"]')).toBeNull();
    expect(fixture.nativeElement.querySelector('textarea')).toBeNull();

    const back = firstValueFrom(
      router.events.pipe(
        filter((event) => event instanceof NavigationEnd),
        take(1),
      ),
    );
    fixture.nativeElement.querySelector('header a').click();
    await back;
    flushList();
    await fixture.whenStable();
    expect(router.url).toBe('/simulations');
    expect(fixture.nativeElement.querySelector('h1')?.textContent).toBe('Simulări');
    http.verify();
  });
});
