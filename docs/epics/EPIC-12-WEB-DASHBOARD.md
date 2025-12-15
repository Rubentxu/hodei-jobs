# EPIC-12: Hodei Web Dashboard

**Versión**: 3.1  
**Fecha**: 2025-12-15  
**Estado**: En Progreso (Sprint 1 Completado)  
**Prioridad**: Alta  
**Diseños Base**: `/web-stich/` (9 pantallas responsive)  
**Tests**: 104 pasando

---

## 📋 Resumen Ejecutivo

Crear un **dashboard web moderno** para Hodei Jobs Platform usando **React/TypeScript + gRPC-Web** con el stack de Buf (Connect), basado en los diseños existentes en `web-stich/`.

### ¿Por qué React/TypeScript + gRPC-Web?

| Aspecto | Ventaja |
|---------|---------|
| **Ecosistema Maduro** | TanStack Query, Tailwind 4.x, Material Symbols |
| **Type Safety** | Connect genera clientes 100% tipados desde .proto |
| **Contratación** | Pool de talento React/TS enorme vs Rust/WASM |
| **DX (Developer Experience)** | HMR instantáneo con Vite (vs 1-4s compilación WASM) |
| **Streaming** | Connect soporta gRPC streaming nativo en browser |
| **Runtime** | Bun como motor JS/TS (más rápido que Node) |

### Pantallas Identificadas (desde web-stich/)

| Pantalla | Archivo | API gRPC Principal |
|----------|---------|-------------------|
| Dashboard Overview | `desktop_dashboard_overview/` | `MetricsService`, `JobExecutionService` |
| Job History List | `desktop_job_history_list/` | `JobExecutionService.GetJob` |
| Job Details View | `desktop_job_details_view/` | `JobExecutionService`, `LogStreamService` |
| Job Scheduling Form | `desktop_job_scheduling_form/` | `JobExecutionService.QueueJob`, `SchedulerService` |
| Real-time Log Stream | `desktop_real-time_job_log_stream/` | `LogStreamService.SubscribeLogs` |
| Metrics Dashboard | `desktop_metrics_dashboard/` | `MetricsService` |
| Providers List | `untitled_screen_2/` | `ProviderManagementService.ListProviders` |
| Provider Details | `untitled_screen_3/` | `ProviderManagementService.GetProvider` |
| New Provider Form | `untitled_screen_1/` | `ProviderManagementService.RegisterProvider` |

---

## 🏗️ Stack Tecnológico

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                           HODEI WEB DASHBOARD                               │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  ┌─────────────────────────────────────────────────────────────────────┐   │
│  │                        FRONTEND (React)                              │   │
│  │                                                                      │   │
│  │  Runtime:       Bun 1.1+ (JS/TS engine)                              │   │
│  │  Framework:     React 18 + TypeScript 5                              │   │
│  │  Build:         Vite 5 (HMR instantáneo)                             │   │
│  │  Routing:       React Router v6                                      │   │
│  │  State:         TanStack Query v5 (async state management)           │   │
│  │  Styling:       Tailwind CSS 4.x (basado en diseños web-stich)       │   │
│  │  Icons:         Material Symbols Outlined                            │   │
│  │  Fonts:         Inter, JetBrains Mono                                │   │
│  │  gRPC Client:   @connectrpc/connect-web                              │   │
│  │  Testing:       @testing-library/react + Vitest                      │   │
│  │                                                                      │   │
│  └──────────────────────────────┬──────────────────────────────────────┘   │
│                                 │                                           │
│                                 │ HTTP/2 + Connect Protocol                 │
│                                 │ (gRPC-Web compatible)                     │
│                                 ▼                                           │
│  ┌─────────────────────────────────────────────────────────────────────┐   │
│  │                        BACKEND (Rust/Tonic)                          │   │
│  │                                                                      │   │
│  │  gRPC Server:   Tonic + tonic-web (gRPC-Web support)                 │   │
│  │  Services:      JobExecution, WorkerAgent, LogStream, Metrics,       │   │
│  │                 ProviderManagement, Scheduler                        │   │
│  │  Proto:         Buf para generación de código                        │   │
│  │                                                                      │   │
│  └─────────────────────────────────────────────────────────────────────┘   │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

### Dependencias Frontend

```json
{
  "dependencies": {
    "react": "^18.3.0",
    "react-dom": "^18.3.0",
    "react-router-dom": "^6.28.0",
    "@tanstack/react-query": "^5.62.0",
    "@connectrpc/connect": "^1.6.1",
    "@connectrpc/connect-web": "^1.6.1",
    "@bufbuild/protobuf": "^2.2.0",
    "tailwindcss": "^4.0.0",
    "clsx": "^2.1.0",
    "date-fns": "^3.6.0"
  },
  "devDependencies": {
    "typescript": "^5.7.0",
    "vite": "^6.0.0",
    "@vitejs/plugin-react": "^4.3.0",
    "@bufbuild/buf": "^1.47.0",
    "@connectrpc/protoc-gen-connect-es": "^1.6.1",
    "@bufbuild/protoc-gen-es": "^2.2.0",
    "@testing-library/react": "^16.1.0",
    "@testing-library/jest-dom": "^6.6.0",
    "vitest": "^2.1.0",
    "@vitest/ui": "^2.1.0",
    "happy-dom": "^15.11.0"
  }
}
```

### Dependencias Backend (Rust)

```toml
# Cargo.toml - añadir a hodei-jobs-grpc
[dependencies]
tonic-web = "0.12"  # gRPC-Web support
tower-http = { version = "0.6", features = ["cors"] }
```

---

## 🎯 Arquitectura de Comunicación

### Connect Protocol (Buf)

Connect es un protocolo moderno que:
1. **Compatible con gRPC** - Usa los mismos .proto
2. **HTTP/1.1 y HTTP/2** - Funciona sin proxy especial
3. **JSON y Binary** - Soporta ambos formatos
4. **Streaming** - Server streaming funciona en browsers

```
┌──────────────┐                    ┌──────────────┐
│   Browser    │                    │  Rust Server │
│   (React)    │                    │   (Tonic)    │
├──────────────┤                    ├──────────────┤
│              │   POST /hodei.     │              │
│  Connect     │   JobExecution     │  tonic-web   │
│  Client      │ ─────────────────▶ │  middleware  │
│              │   Service/QueueJob │              │
│              │                    │              │
│              │   Content-Type:    │              │
│              │   application/     │              │
│              │   connect+proto    │              │
└──────────────┘                    └──────────────┘
```

### Streaming de Logs

```typescript
// React component usando Connect streaming
import { createClient } from "@connectrpc/connect";
import { LogStreamService } from "./gen/hodei_connect";

const client = createClient(LogStreamService, transport);

// Server streaming - logs en tiempo real
for await (const log of client.subscribeLogs({ jobId: "abc-123" })) {
  console.log(log.line);
  // Actualizar UI con cada línea
}
```

---

## 🐳 Entorno de Desarrollo

### Docker Compose para Desarrollo

El archivo `docker-compose.dev.yml` en la raíz del proyecto levanta la infraestructura necesaria:

```yaml
# docker-compose.dev.yml
services:
  postgres:
    image: postgres:16-alpine
    container_name: hodei-postgres
    environment:
      POSTGRES_USER: postgres
      POSTGRES_PASSWORD: postgres
      POSTGRES_DB: hodei
    ports:
      - "5432:5432"
    volumes:
      - postgres_data:/var/lib/postgresql/data
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -U postgres -d hodei"]
      interval: 5s
      timeout: 5s
      retries: 5

volumes:
  postgres_data:
```

### Comandos de Desarrollo

```bash
# 1. Levantar infraestructura (PostgreSQL)
docker compose -f docker-compose.dev.yml up -d

# 2. Verificar que Postgres está listo
docker compose -f docker-compose.dev.yml ps

# 3. Iniciar servidor gRPC (en terminal 1)
export HODEI_DATABASE_URL="postgres://postgres:postgres@localhost:5432/hodei"
HODEI_DEV_MODE=1 HODEI_DOCKER_ENABLED=1 cargo run --bin server -p hodei-jobs-grpc

# 4. Iniciar frontend (en terminal 2)
cd web
bun install
bun run dev   # http://localhost:5173

# 5. Generar clientes gRPC (cuando cambien los .proto)
cd web
bun run generate
```

### Variables de Entorno Frontend

```bash
# web/.env.development
VITE_API_URL=http://localhost:50051
```

---

## 📐 Estructura del Proyecto

```
hodei-job-platform/
├── docker-compose.dev.yml            # 🆕 Infraestructura desarrollo
├── web/                              # 🆕 Frontend React
│   ├── package.json
│   ├── vite.config.ts
│   ├── vitest.config.ts
│   ├── tsconfig.json
│   ├── index.html
│   ├── buf.gen.yaml
│   ├── src/
│   │   ├── main.tsx
│   │   ├── App.tsx
│   │   ├── index.css                 # Tailwind 4.x + custom theme
│   │   ├── gen/                      # 🔄 Generado por Buf
│   │   │   ├── hodei_pb.ts
│   │   │   └── hodei_connect.ts
│   │   ├── lib/
│   │   │   ├── connect.ts
│   │   │   └── query-client.ts
│   │   ├── hooks/
│   │   │   ├── useJobs.ts
│   │   │   ├── useJob.ts
│   │   │   ├── useJobLogs.ts
│   │   │   ├── useProviders.ts
│   │   │   ├── useProvider.ts
│   │   │   └── useMetrics.ts
│   │   ├── components/
│   │   │   ├── layout/
│   │   │   │   ├── Layout.tsx
│   │   │   │   ├── Header.tsx
│   │   │   │   └── BottomNav.tsx
│   │   │   ├── ui/
│   │   │   │   ├── FilterChips.tsx
│   │   │   │   ├── SearchInput.tsx
│   │   │   │   └── ...
│   │   │   ├── jobs/
│   │   │   │   ├── JobCard.tsx
│   │   │   │   ├── JobTimeline.tsx
│   │   │   │   └── ...
│   │   │   ├── logs/
│   │   │   │   ├── LogViewer.tsx
│   │   │   │   └── LogLine.tsx
│   │   │   ├── providers/
│   │   │   │   ├── ProviderCard.tsx
│   │   │   │   └── ...
│   │   │   └── metrics/
│   │   │       ├── StatsCard.tsx
│   │   │       └── ...
│   │   ├── pages/                    # Ver mapeo HTML → React abajo
│   │   │   ├── DashboardPage.tsx
│   │   │   ├── JobHistoryPage.tsx
│   │   │   ├── JobDetailPage.tsx
│   │   │   ├── NewJobPage.tsx
│   │   │   ├── LogStreamPage.tsx
│   │   │   ├── MetricsPage.tsx
│   │   │   ├── ProvidersPage.tsx
│   │   │   ├── ProviderDetailPage.tsx
│   │   │   └── NewProviderPage.tsx
│   │   └── __tests__/
│   │       └── ...
│   └── public/
│       └── favicon.svg
├── web-stich/                        # Diseños HTML originales (referencia)
├── proto/
│   └── *.proto
└── crates/
    └── grpc/
        └── src/
            └── bin/
                └── server.rs         # 🔄 Añadir tonic-web
```

---

## 🗺️ Diseños de Referencia y Conversión a React

### ¿Qué es `web-stich/`?

El directorio `web-stich/` contiene los **diseños HTML estáticos** que sirven como referencia visual y de estilos para el dashboard. Estos archivos:

- **Son mockups/prototipos** - HTML + Tailwind CSS sin lógica
- **Definen el diseño exacto** - Colores, espaciados, tipografía, layout
- **Incluyen capturas de pantalla** - `screen.png` para referencia visual
- **Usan Tailwind CSS** - Las clases ya están definidas, solo hay que convertirlas

### Proceso de Conversión: HTML → React + Tailwind

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                    PROCESO DE CONVERSIÓN POR PANTALLA                       │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  1. ABRIR DISEÑO                                                            │
│     └─▶ Revisar `web-stich/<pantalla>/code.html` en el navegador           │
│     └─▶ Comparar con `web-stich/<pantalla>/screen.png`                     │
│                                                                             │
│  2. ANALIZAR ESTRUCTURA                                                     │
│     └─▶ Identificar secciones principales del HTML                         │
│     └─▶ Identificar componentes reutilizables (cards, buttons, inputs)     │
│     └─▶ Anotar las clases Tailwind usadas                                  │
│                                                                             │
│  3. CREAR COMPONENTE REACT                                                  │
│     └─▶ Crear archivo .tsx en `web/src/pages/` o `web/src/components/`     │
│     └─▶ Convertir HTML a JSX:                                              │
│         • class="..." → className="..."                                     │
│         • for="..." → htmlFor="..."                                         │
│         • onclick → onClick                                                 │
│         • Cerrar tags auto-cerrados: <input /> <img />                     │
│     └─▶ Mantener TODAS las clases Tailwind exactamente igual               │
│                                                                             │
│  4. AÑADIR INTERACTIVIDAD                                                   │
│     └─▶ useState para estado local (filtros, formularios)                  │
│     └─▶ React Router para navegación (Link, useNavigate)                   │
│     └─▶ Handlers para eventos (onClick, onChange, onSubmit)                │
│                                                                             │
│  5. CONECTAR CON API (posterior)                                            │
│     └─▶ Reemplazar datos mock con hooks de TanStack Query                  │
│     └─▶ Usar clientes Connect generados desde .proto                       │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

### Estructura del Directorio `web-stich/`

Los diseños originales están organizados así:

```
web-stich/
├── desktop_dashboard_overview/
│   ├── code.html          # Dashboard principal con stats y recent jobs
│   └── screen.png         # Captura de referencia visual
├── desktop_job_history_list/
│   ├── code.html          # Lista de jobs con filtros y búsqueda
│   └── screen.png
├── desktop_job_details_view/
│   ├── code.html          # Detalle de job con timeline y recursos
│   └── screen.png
├── desktop_job_scheduling_form/
│   ├── code.html          # Formulario para crear nuevo job
│   └── screen.png
├── desktop_real-time_job_log_stream/
│   ├── code.html          # Visor de logs en tiempo real (terminal)
│   └── screen.png
├── desktop_metrics_dashboard/
│   ├── code.html          # Dashboard de métricas con gráficos
│   └── screen.png
├── untitled_screen_1/
│   ├── code.html          # Formulario para crear nuevo provider
│   └── screen.png
├── untitled_screen_2/
│   ├── code.html          # Lista de providers
│   └── screen.png
└── untitled_screen_3/
    ├── code.html          # Detalle de provider
    └── screen.png
```

### Tabla de Migración

| # | Archivo HTML Origen | Ruta React | Componente TSX |
|---|---------------------|------------|----------------|
| 1 | `web-stich/desktop_dashboard_overview/code.html` | `/` | `src/pages/DashboardPage.tsx` |
| 2 | `web-stich/desktop_job_history_list/code.html` | `/jobs` | `src/pages/JobHistoryPage.tsx` |
| 3 | `web-stich/desktop_job_details_view/code.html` | `/jobs/:jobId` | `src/pages/JobDetailPage.tsx` |
| 4 | `web-stich/desktop_job_scheduling_form/code.html` | `/jobs/new` | `src/pages/NewJobPage.tsx` |
| 5 | `web-stich/desktop_real-time_job_log_stream/code.html` | `/jobs/:jobId/logs` | `src/pages/LogStreamPage.tsx` |
| 6 | `web-stich/desktop_metrics_dashboard/code.html` | `/metrics` | `src/pages/MetricsPage.tsx` |
| 7 | `web-stich/untitled_screen_2/code.html` | `/providers` | `src/pages/ProvidersPage.tsx` |
| 8 | `web-stich/untitled_screen_3/code.html` | `/providers/:providerId` | `src/pages/ProviderDetailPage.tsx` |
| 9 | `web-stich/untitled_screen_1/code.html` | `/providers/new` | `src/pages/NewProviderPage.tsx` |

### Reglas de Migración

1. **Abrir el HTML original** - Revisar `web-stich/<pantalla>/code.html` y `screen.png`
2. **Copiar HTML exacto** - No inventar estilos, usar las clases Tailwind del HTML original
3. **Convertir a JSX** - `class` → `className`, cerrar tags, atributos en camelCase
4. **Extraer componentes** - Identificar elementos repetidos y crear componentes reutilizables
5. **Añadir interactividad** - useState, handlers, navegación con React Router
6. **Conectar con API** - Reemplazar datos mock con hooks de TanStack Query + Connect

---

## 🖥️ Análisis de Pantallas (web-stich/)

### Pantalla 1: Dashboard Overview
- **HTML:** `web-stich/desktop_dashboard_overview/code.html`
- **Screenshot:** `web-stich/desktop_dashboard_overview/screen.png`
- **APIs:** `MetricsService.GetSystemMetrics`, `JobExecutionService.GetJob`
- **Componentes:** Header, StatsGrid (4 cards), SystemHealthChart, RecentExecutions, FAB, BottomNav

### Pantalla 2: Job History List
- **HTML:** `web-stich/desktop_job_history_list/code.html`
- **Screenshot:** `web-stich/desktop_job_history_list/screen.png`
- **APIs:** `JobExecutionService.GetJob` (con filtros)
- **Componentes:** SearchInput, FilterChips, JobList (agrupado por fecha)

### Pantalla 3: Job Details View
- **HTML:** `web-stich/desktop_job_details_view/code.html`
- **Screenshot:** `web-stich/desktop_job_details_view/screen.png`
- **APIs:** `JobExecutionService.GetJob`, `LogStreamService.GetLogs`, `JobExecutionService.CancelJob`
- **Componentes:** JobHeader, Tabs, JobTimeline, ResourceCards, LogPreview, ActionBar

### Pantalla 4: Job Scheduling Form
- **HTML:** `web-stich/desktop_job_scheduling_form/code.html`
- **Screenshot:** `web-stich/desktop_job_scheduling_form/screen.png`
- **APIs:** `JobExecutionService.QueueJob`, `ProviderManagementService.ListProviders`
- **Componentes:** JobForm (5 secciones: Core, Environment, Resources, I/O, Preferences)

### Pantalla 5: Real-time Log Stream
- **HTML:** `web-stich/desktop_real-time_job_log_stream/code.html`
- **Screenshot:** `web-stich/desktop_real-time_job_log_stream/screen.png`
- **APIs:** `LogStreamService.SubscribeLogs` (streaming)
- **Componentes:** LogViewer, LogFilters, LogControls

### Pantalla 6: Metrics Dashboard
- **HTML:** `web-stich/desktop_metrics_dashboard/code.html`
- **Screenshot:** `web-stich/desktop_metrics_dashboard/screen.png`
- **APIs:** `MetricsService.GetSystemMetrics`, `MetricsService.GetAggregatedMetrics`
- **Componentes:** TimeRangeSelector, MetricsGrid, JobDistribution, ExecutionTrends, ProvidersList

### Pantalla 7: Providers List
- **HTML:** `web-stich/untitled_screen_2/code.html`
- **Screenshot:** `web-stich/untitled_screen_2/screen.png`
- **APIs:** `ProviderManagementService.ListProviders`, `ProviderManagementService.GetProviderStats`
- **Componentes:** ProviderList, ProviderCard, FilterChips

### Pantalla 8: Provider Details
- **HTML:** `web-stich/untitled_screen_3/code.html`
- **Screenshot:** `web-stich/untitled_screen_3/screen.png`
- **APIs:** `ProviderManagementService.GetProvider`, `ProviderManagementService.UpdateProvider`
- **Componentes:** ProviderHeader, ProviderStats, ConfigSection, HealthLog

### Pantalla 9: New Provider Form
- **HTML:** `web-stich/untitled_screen_1/code.html`
- **Screenshot:** `web-stich/untitled_screen_1/screen.png`
- **APIs:** `ProviderManagementService.RegisterProvider`
- **Componentes:** ProviderForm (Type selector, Configuration, Capabilities)

---

## 📝 Historias de Usuario

> ⚠️ **IMPORTANTE: Diseños Finales Obligatorios**
> 
> Los archivos HTML en `web-stich/` son los **diseños finales aprobados**. No se permite ninguna desviación visual.
> Cada historia de usuario que implemente una pantalla **DEBE**:
> 1. Usar exactamente las clases Tailwind CSS del HTML original
> 2. Mantener la misma estructura de elementos
> 3. Respetar colores, espaciados, tipografía y layout
> 4. Comparar visualmente con el `screen.png` correspondiente antes de dar por completada

### Epic 12.1: Infraestructura y Setup

#### US-12.1.1: Scaffold del Proyecto con Bun y Vite

**Como** desarrollador  
**Quiero** un proyecto React/TypeScript configurado con Bun y Vite  
**Para** desarrollar el dashboard con máximo rendimiento

**Pantalla:** Todas | **APIs:** N/A (setup)

**Criterios de Aceptación:**
- [x] Proyecto creado con `bun create vite web --template react-ts`
- [x] TypeScript 5.7+ con strict mode
- [x] Tailwind CSS 4.x con colores custom: `primary: #2b6cee`, `background-dark: #101622`, `card-dark: #18202F`
- [x] Fuentes Inter y JetBrains Mono
- [x] Material Symbols Outlined
- [x] ESLint + Prettier configurados
- [x] Vitest + Testing Library configurados

**Test:** ✅ `bun run test` ejecuta 104 tests correctamente

**Estimación:** 3 puntos | **Estado:** ✅ COMPLETADO

---

#### US-12.1.2: Configuración de Buf y Connect

**Como** desarrollador  
**Quiero** generar clientes TypeScript desde los .proto  
**Para** tener type-safety completo con el backend

**Pantalla:** Todas | **APIs:** Todas (generación)

**Criterios de Aceptación:**
- [x] `buf.gen.yaml` con plugins es y connect-es
- [x] Clientes generados para: JobExecutionService, WorkerAgentService, LogStreamService, MetricsService, SchedulerService (usando `hodei_all_in_one.proto`)
- [x] Script `bun run generate` funcional
- [x] Transport Connect configurado (`web/src/lib/connect.ts`)
- [x] Clientes exportados: `jobClient`, `logClient`, `metricsClient`, `schedulerClient`

**Test:** ✅ Clientes generados en `web/src/gen/`

**Estimación:** 3 puntos | **Estado:** ✅ COMPLETADO

---

#### US-12.1.3: TanStack Query y Hooks Base

**Como** desarrollador  
**Quiero** hooks personalizados para cada entidad  
**Para** gestionar estado asíncrono eficientemente

**Pantalla:** Todas | **APIs:** Todas

**Criterios de Aceptación:**
- [x] QueryClient con defaults apropiados (`web/src/lib/query-client.ts`)
- [x] Hooks: `useJobs()`, `useJob(id)`, `useProviders()`, `useProvider(id)`, `useMetrics()`, `useJobLogs()` (`web/src/hooks/`)
- [ ] DevTools en desarrollo (opcional)
- [ ] Error boundaries configurados (opcional)

**Test:** ✅ 9 tests de hooks pasando en `useJobs.test.tsx`

**Estimación:** 3 puntos | **Estado:** ✅ COMPLETADO

---

#### US-12.1.4: Backend tonic-web Support

**Como** sistema  
**Quiero** que el servidor gRPC soporte gRPC-Web  
**Para** que el browser conecte directamente

**Pantalla:** Todas | **APIs:** Todas (transport)

**Criterios de Aceptación:**
- [x] `tonic-web = "0.14"` en Cargo.toml (workspace)
- [x] Middleware configurado en server.rs (`GrpcWebLayer::new()`)
- [x] CORS para localhost:5173 (`CorsLayer` con `allow_origin(Any)`)
- [ ] Streaming funciona desde browser (pendiente test E2E)

**Test:** ✅ Backend compila con soporte gRPC-Web

**Estimación:** 3 puntos | **Estado:** ✅ COMPLETADO

---

### Epic 12.2: Layout y Navegación

#### US-12.2.1: Layout con Bottom Navigation

**Como** usuario  
**Quiero** navegación bottom nav en mobile  
**Para** moverme fácilmente entre secciones

**Diseño de Referencia:**
- **HTML:** `web-stich/desktop_dashboard_overview/code.html`
- **Screenshot:** `web-stich/desktop_dashboard_overview/screen.png`

**APIs:** N/A | **Componentes:** `Layout.tsx`, `Header.tsx`, `BottomNav.tsx`

**Criterios de Aceptación:**
- [x] ✅ **FIDELIDAD:** Implementación idéntica al HTML de `web-stich/desktop_dashboard_overview/code.html`
- [x] Header sticky con blur (`backdrop-blur-md`) - líneas 53-61 del HTML
- [x] Avatar con ring, search y notifications
- [x] Bottom nav con 4 items - líneas 246-259 del HTML
- [x] Active state con `text-primary` y filled icon
- [x] Clases Tailwind exactas del diseño original
- [x] Safe area padding (`pb-safe`)

**Test:** ✅ 5 tests de BottomNav pasando

**Estimación:** 3 puntos | **Estado:** ✅ COMPLETADO

---

### Epic 12.3: Dashboard Overview (Pantalla 1)

> **Diseño Final:** `web-stich/desktop_dashboard_overview/code.html` + `screen.png`

#### US-12.3.1: Stats Cards Grid

**Como** usuario  
**Quiero** ver métricas principales en cards  
**Para** visión rápida del estado del sistema

**Diseño:** `web-stich/desktop_dashboard_overview/code.html` (líneas 62-95)

**APIs:** `MetricsService.GetSystemMetrics` | **Componentes:** `StatsCard.tsx`, `JobStats.tsx`

**Criterios de Aceptación:**
- [x] ✅ **FIDELIDAD:** Clases Tailwind idénticas al HTML original
- [x] Grid 2x2: Total Jobs, Running (animado), Failed, Success
- [x] Colores exactos: `bg-primary/10`, `bg-error/10`, `bg-success/10`
- [x] Icon spinning para Running (`animate-spin`)
- [ ] Auto-refresh cada 10s (pendiente conexión API real)

**Test:** ✅ Tests de DashboardPage pasando

**Estimación:** 3 puntos | **Estado:** ✅ COMPLETADO

---

#### US-12.3.2: System Health Chart

**Como** usuario  
**Quiero** ver gráfico de salud del sistema  
**Para** monitorear CPU Load y uptime

**Diseño:** `web-stich/desktop_dashboard_overview/code.html` (líneas 97-145)

**APIs:** `MetricsService.GetSystemMetrics` | **Componentes:** `SystemHealthChart.tsx`

**Criterios de Aceptación:**
- [ ] ✅ **FIDELIDAD:** Estructura y clases idénticas al HTML original
- [ ] Card con "System Health", "CPU Load (24h)"
- [ ] Uptime % en verde (`text-success`)
- [ ] SVG line chart con gradient
- [ ] Active Nodes con pulse (`animate-ping`), Queue status

**Test:** `render(<SystemHealthChart uptime={98} />)` muestra "98%"

**Estimación:** 5 puntos

---

#### US-12.3.3: Recent Executions List

**Como** usuario  
**Quiero** ver ejecuciones recientes  
**Para** acceder rápidamente a jobs relevantes

**Diseño:** `web-stich/desktop_dashboard_overview/code.html` (líneas 147-244)

**APIs:** `JobExecutionService.GetJob` | **Componentes:** `JobCard.tsx`, `RecentExecutions.tsx`

**Criterios de Aceptación:**
- [ ] ✅ **FIDELIDAD:** JobCard idéntico al diseño HTML
- [ ] Header con "See All" button
- [ ] Job cards: icon, ID, nombre, status badge, tiempo
- [ ] Status badges con clases exactas del HTML
- [ ] Botón replay para completados/fallidos
- [ ] Touch feedback (`active:scale-[0.98]`)

**Test:** `render(<JobCard status="RUNNING" />)` tiene icon con `animate-spin`

**Estimación:** 5 puntos

---

### Epic 12.4: Job History List (Pantalla 2)

> **Diseño Final:** `web-stich/desktop_job_history_list/code.html` + `screen.png`

#### US-12.4.1: Job History con Filtros

**Como** usuario  
**Quiero** historial de jobs con filtros  
**Para** encontrar jobs específicos

**Diseño:** `web-stich/desktop_job_history_list/code.html` (completo)

**APIs:** `JobExecutionService.GetJob` | **Componentes:** `JobHistoryPage.tsx`, `FilterChips.tsx`, `JobList.tsx`

**Criterios de Aceptación:**
- [ ] ✅ **FIDELIDAD:** Implementación idéntica al HTML de `desktop_job_history_list/code.html`
- [ ] Search bar con icono (líneas 63-70)
- [ ] Filter chips: All, Running, Failed, Succeeded, Pending (líneas 72-92)
- [ ] Jobs agrupados por TODAY/YESTERDAY con separadores
- [ ] JobCard con progress bar para RUNNING (líneas 100-135)
- [ ] Infinite scroll o paginación

**Test:** Click en "Failed" filtra solo jobs fallidos

**Estimación:** 5 puntos

---

### Epic 12.5: Job Details View (Pantalla 3)

> **Diseño Final:** `web-stich/desktop_job_details_view/code.html` + `screen.png`

#### US-12.5.1: Job Header y Tabs

**Como** usuario  
**Quiero** ver información principal del job  
**Para** entender su contexto y estado

**Diseño:** `web-stich/desktop_job_details_view/code.html` (líneas 50-95)

**APIs:** `JobExecutionService.GetJob` | **Componentes:** `JobDetailPage.tsx`, `JobHeader.tsx`, `Tabs.tsx`

**Criterios de Aceptación:**
- [ ] ✅ **FIDELIDAD:** Estructura idéntica al HTML de `desktop_job_details_view/code.html`
- [ ] Job ID grande con status badge
- [ ] Metadata chips con clases exactas del HTML
- [ ] Tabs segmented control: Overview, Config, Logs, Res

**Test:** Status "RUNNING" muestra dot con clase `animate-pulse`

**Estimación:** 3 puntos

---

#### US-12.5.2: Timeline de Ejecución

**Como** usuario  
**Quiero** ver timeline de ejecución  
**Para** entender en qué fase está el job

**Diseño:** `web-stich/desktop_job_details_view/code.html` (líneas 97-150)

**APIs:** `JobExecutionService.GetJob` | **Componentes:** `JobTimeline.tsx`

**Criterios de Aceptación:**
- [ ] ✅ **FIDELIDAD:** Timeline idéntico al diseño HTML
- [ ] Timeline vertical con línea conectora
- [ ] Steps: Queued, Image Pulled, Running, Cleanup
- [ ] Dots con colores exactos del HTML
- [ ] Current step con shadow glow
- [ ] Timestamps a la derecha

**Test:** `currentStep={2}` resalta "Running Script" en primary

**Estimación:** 5 puntos

---

#### US-12.5.3: Live Resources y Log Preview

**Como** usuario  
**Quiero** ver recursos y logs preview  
**Para** monitorear consumo del job

**Diseño:** `web-stich/desktop_job_details_view/code.html` (líneas 152-220)

**APIs:** `MetricsService`, `LogStreamService.GetLogs` | **Componentes:** `ResourceCard.tsx`, `LogPreview.tsx`

**Criterios de Aceptación:**
- [ ] ✅ **FIDELIDAD:** Cards y log preview idénticos al HTML
- [ ] Grid 2 cols: CPU, Memory con progress bars
- [ ] Log preview terminal style con clases exactas
- [ ] Colores por nivel según HTML: INFO (blue), WARN (yellow), ERROR (red)

**Test:** `render(<ResourceCard type="cpu" value={42} />)` muestra "42%"

**Estimación:** 5 puntos

---

#### US-12.5.4: Cancel Job Action

**Como** usuario  
**Quiero** cancelar jobs en ejecución  
**Para** detener jobs innecesarios

**Diseño:** `web-stich/desktop_job_details_view/code.html` (líneas 222-255)

**APIs:** `JobExecutionService.CancelJob` | **Componentes:** `JobActions.tsx`

**Criterios de Aceptación:**
- [ ] ✅ **FIDELIDAD:** Footer sticky idéntico al HTML
- [ ] Botones SSH Access y Cancel Job con clases exactas
- [ ] Confirmación antes de cancelar
- [ ] Loading state durante cancelación

**Test:** Click Cancel → Confirm llama `cancelJob`

**Estimación:** 3 puntos

---

### Epic 12.6: Real-time Log Stream (Pantalla 5)

> **Diseño Final:** `web-stich/desktop_real-time_job_log_stream/code.html` + `screen.png`

#### US-12.6.1: Full Log Viewer con Streaming

**Como** usuario  
**Quiero** ver logs completos en tiempo real  
**Para** debuggear y monitorear ejecución

**Diseño:** `web-stich/desktop_real-time_job_log_stream/code.html` (completo)

**APIs:** `LogStreamService.SubscribeLogs` | **Componentes:** `LogStreamPage.tsx`, `LogViewer.tsx`, `LogLine.tsx`

**Criterios de Aceptación:**
- [ ] ✅ **FIDELIDAD:** Implementación idéntica al HTML de `desktop_real-time_job_log_stream/code.html`
- [ ] Header con Build #, nombre, status live indicator (líneas 69-86)
- [ ] Job meta info bar (líneas 89-93)
- [ ] Terminal full-height con fondo `bg-console-bg` (líneas 121-220)
- [ ] Log entries con timestamps, niveles coloreados según HTML
- [ ] WARN con `border-l-2 border-yellow-500/50 bg-yellow-500/5`
- [ ] ERROR con `border-l-2 border-red-500/50 bg-red-500/5`
- [ ] Streaming real-time via Connect

**Test:** Streaming añade logs y auto-scrollea

**Estimación:** 8 puntos

---

#### US-12.6.2: Log Filters y Controls

**Como** usuario  
**Quiero** filtrar y exportar logs  
**Para** encontrar información específica

**Diseño:** `web-stich/desktop_real-time_job_log_stream/code.html` (líneas 94-118, 227-244)

**APIs:** N/A (client-side) | **Componentes:** `LogFilters.tsx`, `LogControls.tsx`

**Criterios de Aceptación:**
- [ ] ✅ **FIDELIDAD:** Search bar y filter chips idénticos al HTML
- [ ] Search "Search logs (grep)..." (líneas 95-101)
- [ ] Filter chips: All Logs, INFO, WARN, ERROR (líneas 104-117)
- [ ] Action bar: Pause/Resume, Clear, Settings (líneas 228-243)
- [ ] Scroll to bottom FAB (líneas 222-226)

**Test:** Filtrar por "ERROR" muestra solo líneas de error

**Estimación:** 5 puntos

---

### Epic 12.7: Job Scheduling Form (Pantalla 4)

> **Diseño Final:** `web-stich/desktop_job_scheduling_form/code.html` + `screen.png`

#### US-12.7.1: New Job Form Completo

**Como** usuario  
**Quiero** crear jobs con todas las opciones  
**Para** ejecutar tareas personalizadas

**Diseño:** `web-stich/desktop_job_scheduling_form/code.html` (completo)

**APIs:** `JobExecutionService.QueueJob`, `ProviderManagementService.ListProviders` | **Componentes:** `NewJobPage.tsx`, `JobForm.tsx`

**Criterios de Aceptación:**
- [ ] ✅ **FIDELIDAD:** Formulario idéntico al HTML de `desktop_job_scheduling_form/code.html`
- [ ] Header con close y Reset (líneas 56-64)
- [ ] Core Execution: Command Type select, Script textarea (líneas 68-96)
- [ ] Environment & Image: Container Image, Env Vars add/remove (líneas 98-133)
- [ ] Resources: CPU, Memory, Storage, Timeout inputs, GPU toggle, Architecture (líneas 135-175)
- [ ] I/O & Constraints: Job Inputs, Outputs, Placement (líneas 177-217)
- [ ] Preferences: Provider, Region, Priority, Retry toggle (líneas 219-280)
- [ ] Footer sticky con "Schedule Job" button (líneas 283-288)

**Test:** Submit con command vacío muestra error de validación

**Estimación:** 8 puntos

---

### Epic 12.8: Metrics Dashboard (Pantalla 6)

> **Diseño Final:** `web-stich/desktop_metrics_dashboard/code.html` + `screen.png`

#### US-12.8.1: System Overview con Time Range

**Como** administrador  
**Quiero** métricas con rangos de tiempo  
**Para** analizar tendencias

**Diseño:** `web-stich/desktop_metrics_dashboard/code.html` (completo)

**APIs:** `MetricsService.GetAggregatedMetrics` | **Componentes:** `MetricsPage.tsx`, `TimeRangeSelector.tsx`, `JobDistribution.tsx`, `ExecutionTrends.tsx`

**Criterios de Aceptación:**
- [ ] ✅ **FIDELIDAD:** Dashboard idéntico al HTML de `desktop_metrics_dashboard/code.html`
- [ ] Header con menu y avatar (líneas 65-75)
- [ ] Time range selector: 1H, 24H, 7D, 30D (líneas 79-98)
- [ ] KPI Grid 2x2: Total Jobs, Success %, Avg Duration, CPU Load (líneas 100-152)
- [ ] Job Distribution stacked bar (líneas 154-194)
- [ ] Execution Trends bar chart (líneas 196-225)
- [ ] Active Providers list con CPU/Memory bars (líneas 227-335)

**Test:** Cambiar a "7D" actualiza métricas

**Estimación:** 8 puntos

---

### Epic 12.9: Providers Management (Pantallas 7, 8, 9)

> **Diseños Finales:**
> - Lista: `web-stich/untitled_screen_2/code.html` + `screen.png`
> - Detalle: `web-stich/untitled_screen_3/code.html` + `screen.png`
> - Nuevo: `web-stich/untitled_screen_1/code.html` + `screen.png`

#### US-12.9.1: Providers List

**Como** administrador  
**Quiero** ver lista de providers  
**Para** monitorear infraestructura

**Diseño:** `web-stich/untitled_screen_2/code.html` (completo)

**APIs:** `ProviderManagementService.ListProviders` | **Componentes:** `ProvidersPage.tsx`, `ProviderCard.tsx`

**Criterios de Aceptación:**
- [ ] ✅ **FIDELIDAD:** Lista idéntica al HTML de `untitled_screen_2/code.html`
- [ ] Header con título y add button (líneas 40-47)
- [ ] Search bar y filter chips (líneas 48-72)
- [ ] Provider cards con estructura exacta del HTML (líneas 75-228)
- [ ] Status badges: Active (verde), Unhealthy (rojo), Overloaded (naranja)
- [ ] Bottom nav con 4 items (líneas 230-249)

**Test:** Provider unhealthy muestra warning message

**Estimación:** 5 puntos

---

#### US-12.9.2: Provider Details

**Como** administrador  
**Quiero** ver detalles de un provider  
**Para** gestionar su configuración

**Diseño:** `web-stich/untitled_screen_3/code.html` (completo)

**APIs:** `ProviderManagementService.GetProvider` | **Componentes:** `ProviderDetailPage.tsx`, `ProviderHealth.tsx`

**Criterios de Aceptación:**
- [ ] ✅ **FIDELIDAD:** Detalle idéntico al HTML de `untitled_screen_3/code.html`
- [ ] iOS-style status bar y nav (líneas 34-49)
- [ ] Provider header con avatar, nombre, status (líneas 51-79)
- [ ] Stats grid: Current Jobs, Health Score (líneas 67-79)
- [ ] Action buttons: Mark Healthy, Maintenance, Shutdown (líneas 80-99)
- [ ] Configuration list (líneas 100-120)
- [ ] Capabilities tags (líneas 121-140)
- [ ] Health Log timeline (líneas 141-187)
- [ ] Raw Config JSON viewer (líneas 189-200)

**Test:** Click "Shutdown" llama `DisableProvider`

**Estimación:** 5 puntos

---

#### US-12.9.3: New Provider Form

**Como** administrador  
**Quiero** crear nuevos providers  
**Para** expandir infraestructura

**Diseño:** `web-stich/untitled_screen_1/code.html` (completo)

**APIs:** `ProviderManagementService.RegisterProvider` | **Componentes:** `NewProviderPage.tsx`, `ProviderForm.tsx`

**Criterios de Aceptación:**
- [ ] ✅ **FIDELIDAD:** Formulario idéntico al HTML de `untitled_screen_1/code.html`
- [ ] Nav con back y Save (líneas 60-66)
- [ ] Provider Type selector: Docker, K8s, Azure VM (líneas 68-93)
- [ ] Configuration: Name, Endpoint URL, API Token, Labels (líneas 94-125)
- [ ] Capabilities: Max Memory slider, Max vCPUs slider (líneas 126-156)
- [ ] Toggles: GPU Support, Secure Boot (líneas 158-172)
- [ ] Footer con Create Provider y Cancel (líneas 179-187)

**Test:** Submit con nombre vacío muestra error

**Estimación:** 5 puntos

---

## 🗓️ Plan de Implementación

### Sprint 1: Foundation (Semana 1-2) - 15 puntos

| Historia | Puntos | HTML Origen | Archivo React |
|----------|--------|-------------|---------------|
| US-12.1.1: Scaffold Bun + Vite | 3 | N/A | `web/package.json`, `vite.config.ts` |
| US-12.1.2: Buf + Connect | 3 | N/A | `web/buf.gen.yaml`, `src/lib/connect.ts` |
| US-12.1.3: TanStack Query + Hooks | 3 | N/A | `src/hooks/*.ts` |
| US-12.1.4: tonic-web Backend | 3 | N/A | `crates/grpc/src/bin/server.rs` |
| US-12.2.1: Layout + Bottom Nav | 3 | `desktop_dashboard_overview/` | `src/components/layout/*.tsx` |

**Entregables Sprint 1:**
- [ ] `docker-compose.dev.yml` funcional
- [ ] Proyecto web/ con `bun dev` funcionando
- [ ] Clientes gRPC generados en `src/gen/`
- [ ] Layout base con BottomNav

### Sprint 2: Dashboard + Jobs List (Semana 3-4) - 16 puntos

| Historia | Puntos | HTML Origen | Archivo React |
|----------|--------|-------------|---------------|
| US-12.3.1: Stats Cards Grid | 3 | `desktop_dashboard_overview/` | `src/pages/DashboardPage.tsx` |
| US-12.3.2: System Health Chart | 5 | `desktop_dashboard_overview/` | `src/components/metrics/SystemHealthChart.tsx` |
| US-12.3.3: Recent Executions | 5 | `desktop_dashboard_overview/` | `src/components/jobs/RecentExecutions.tsx` |
| US-12.4.1: Job History + Filtros | 5 | `desktop_job_history_list/` | `src/pages/JobHistoryPage.tsx` |

**Entregables Sprint 2:**
- [ ] Pantalla 1 migrada: `/` → `DashboardPage.tsx`
- [ ] Pantalla 2 migrada: `/jobs` → `JobHistoryPage.tsx`
- [ ] Componentes: `StatsCard`, `JobCard`, `FilterChips`, `SearchInput`

### Sprint 3: Job Details + Logs (Semana 5-6) - 16 puntos

| Historia | Puntos | HTML Origen | Archivo React |
|----------|--------|-------------|---------------|
| US-12.5.1: Job Header + Tabs | 3 | `desktop_job_details_view/` | `src/pages/JobDetailPage.tsx` |
| US-12.5.2: Timeline Ejecución | 5 | `desktop_job_details_view/` | `src/components/jobs/JobTimeline.tsx` |
| US-12.5.3: Resources + Log Preview | 5 | `desktop_job_details_view/` | `src/components/jobs/ResourceCards.tsx` |
| US-12.5.4: Cancel Job | 3 | `desktop_job_details_view/` | `src/hooks/useCancelJob.ts` |

**Entregables Sprint 3:**
- [ ] Pantalla 3 migrada: `/jobs/:jobId` → `JobDetailPage.tsx`
- [ ] Componentes: `JobTimeline`, `ResourceCards`, `LogPreview`

### Sprint 4: Log Streaming + Job Form (Semana 7-8) - 21 puntos

| Historia | Puntos | HTML Origen | Archivo React |
|----------|--------|-------------|---------------|
| US-12.6.1: Full Log Viewer | 8 | `desktop_real-time_job_log_stream/` | `src/pages/LogStreamPage.tsx` |
| US-12.6.2: Log Filters + Controls | 5 | `desktop_real-time_job_log_stream/` | `src/components/logs/LogControls.tsx` |
| US-12.7.1: New Job Form | 8 | `desktop_job_scheduling_form/` | `src/pages/NewJobPage.tsx` |

**Entregables Sprint 4:**
- [ ] Pantalla 4 migrada: `/jobs/new` → `NewJobPage.tsx`
- [ ] Pantalla 5 migrada: `/jobs/:jobId/logs` → `LogStreamPage.tsx`
- [ ] Streaming de logs funcionando con Connect

### Sprint 5: Metrics + Providers (Semana 9-10) - 23 puntos

| Historia | Puntos | HTML Origen | Archivo React |
|----------|--------|-------------|---------------|
| US-12.8.1: Metrics Dashboard | 8 | `desktop_metrics_dashboard/` | `src/pages/MetricsPage.tsx` |
| US-12.9.1: Providers List | 5 | `untitled_screen_2/` | `src/pages/ProvidersPage.tsx` |
| US-12.9.2: Provider Details | 5 | `untitled_screen_3/` | `src/pages/ProviderDetailPage.tsx` |
| US-12.9.3: New Provider Form | 5 | `untitled_screen_1/` | `src/pages/NewProviderPage.tsx` |

**Entregables Sprint 5:**
- [ ] Pantalla 6 migrada: `/metrics` → `MetricsPage.tsx`
- [ ] Pantalla 7 migrada: `/providers` → `ProvidersPage.tsx`
- [ ] Pantalla 8 migrada: `/providers/:providerId` → `ProviderDetailPage.tsx`
- [ ] Pantalla 9 migrada: `/providers/new` → `NewProviderPage.tsx`

### Resumen Total

| Sprint | Puntos | Pantallas HTML → React |
|--------|--------|------------------------|
| Sprint 1 | 15 | Setup + Layout base |
| Sprint 2 | 16 | `desktop_dashboard_overview/`, `desktop_job_history_list/` |
| Sprint 3 | 16 | `desktop_job_details_view/` |
| Sprint 4 | 21 | `desktop_real-time_job_log_stream/`, `desktop_job_scheduling_form/` |
| Sprint 5 | 23 | `desktop_metrics_dashboard/`, `untitled_screen_1/2/3/` |
| **Total** | **91** | **9 pantallas migradas** |

---

## 📁 Archivos Creados (Progreso)

### Infraestructura
- [x] `docker-compose.dev.yml` - PostgreSQL para desarrollo

### Proyecto Web (Parcial - Sprint 1 en progreso)
- [x] `web/package.json` - Dependencias
- [x] `web/tsconfig.json` - TypeScript config
- [x] `web/vite.config.ts` - Vite + Tailwind
- [x] `web/vitest.config.ts` - Testing config
- [x] `web/index.html` - Entry point
- [x] `web/src/index.css` - Tailwind 4.x + custom theme
- [x] `web/src/main.tsx` - React entry
- [x] `web/src/App.tsx` - Router setup

### Componentes Layout
- [x] `web/src/components/layout/Layout.tsx`
- [x] `web/src/components/layout/BottomNav.tsx`
- [x] `web/src/components/layout/Header.tsx`

### Componentes UI
- [x] `web/src/components/ui/FilterChips.tsx`
- [x] `web/src/components/ui/SearchInput.tsx`

### Componentes Jobs
- [x] `web/src/components/jobs/JobCard.tsx`

### Páginas (Migración HTML→React)
- [x] `web/src/pages/DashboardPage.tsx` ← `desktop_dashboard_overview/code.html`
- [x] `web/src/pages/JobHistoryPage.tsx` ← `desktop_job_history_list/code.html`
- [x] `web/src/pages/JobDetailPage.tsx` ← `desktop_job_details_view/code.html`
- [x] `web/src/pages/NewJobPage.tsx` ← `desktop_job_scheduling_form/code.html`
- [x] `web/src/pages/LogStreamPage.tsx` ← `desktop_real-time_job_log_stream/code.html`
- [x] `web/src/pages/MetricsPage.tsx` ← `desktop_metrics_dashboard/code.html`
- [x] `web/src/pages/ProvidersPage.tsx` ← `untitled_screen_2/code.html`
- [x] `web/src/pages/ProviderDetailPage.tsx` ← `untitled_screen_3/code.html`
- [x] `web/src/pages/NewProviderPage.tsx` ← `untitled_screen_1/code.html`

### Infraestructura Completada
- [x] `web/buf.gen.yaml` - Configuración Buf para generación de clientes
- [x] `web/src/lib/connect.ts` - Cliente Connect configurado
- [x] `web/src/lib/query-client.ts` - QueryClient TanStack Query
- [x] `web/src/hooks/useJobs.ts` - Hooks para jobs (mock data)
- [x] `web/src/hooks/useProviders.ts` - Hooks para providers (mock data)
- [x] `web/src/hooks/useMetrics.ts` - Hooks para métricas (mock data)
- [x] `web/src/hooks/useJobLogs.ts` - Hooks para logs (mock data)
- [x] `web/src/__tests__/setup.ts` - Test setup con mocks
- [x] Backend: tonic-web en `server.rs` con CORS

### Clientes gRPC Generados
- [x] `web/src/gen/hodei_all_in_one_pb.ts` - Tipos protobuf
- [x] `web/src/gen/hodei_all_in_one_connect.ts` - Servicios Connect

### Pendiente (Sprint 2)
- [ ] Conectar hooks con API real (reemplazar mock data por clientes gRPC)
- [ ] Tests E2E con Playwright
- [ ] DevTools de TanStack Query en desarrollo

---

## 🔧 Configuración Técnica

### buf.gen.yaml

```yaml
version: v2
plugins:
  - local: protoc-gen-es
    out: src/gen
    opt: target=ts
  - local: protoc-gen-connect-es
    out: src/gen
    opt: target=ts
inputs:
  - directory: ../proto
```

### vite.config.ts

```typescript
import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import tailwindcss from '@tailwindcss/vite'
import path from 'path'

export default defineConfig({
  plugins: [react(), tailwindcss()],
  resolve: {
    alias: {
      '@': path.resolve(__dirname, './src'),
    },
  },
  server: {
    port: 5173,
    proxy: {
      '/hodei.': {
        target: 'http://localhost:50051',
        changeOrigin: true,
      },
    },
  },
})
```

### vitest.config.ts

```typescript
import { defineConfig } from 'vitest/config'
import react from '@vitejs/plugin-react'
import path from 'path'

export default defineConfig({
  plugins: [react()],
  test: {
    environment: 'happy-dom',
    globals: true,
    setupFiles: ['./src/__tests__/setup.ts'],
    include: ['src/**/*.test.{ts,tsx}'],
    coverage: {
      provider: 'v8',
      reporter: ['text', 'json', 'html'],
      exclude: ['src/gen/**', 'src/__tests__/**'],
    },
  },
  resolve: {
    alias: {
      '@': path.resolve(__dirname, './src'),
    },
  },
})
```

### tailwind.config.ts (Tailwind 4.x)

```typescript
import type { Config } from 'tailwindcss'

export default {
  content: ['./index.html', './src/**/*.{js,ts,jsx,tsx}'],
  darkMode: 'class',
  theme: {
    extend: {
      colors: {
        primary: '#2b6cee',
        'background-light': '#f6f6f8',
        'background-dark': '#101622',
        'card-dark': '#18202F',
        'card-light': '#ffffff',
        'surface-dark': '#1c2533',
        'surface-highlight': '#283245',
      },
      fontFamily: {
        display: ['Inter', 'sans-serif'],
        mono: ['JetBrains Mono', 'monospace'],
      },
    },
  },
  plugins: [],
} satisfies Config
```

### Connect Client Setup

```typescript
// src/lib/connect.ts
import { createConnectTransport } from "@connectrpc/connect-web";
import { createClient } from "@connectrpc/connect";
import { 
  JobExecutionService, 
  LogStreamService,
  MetricsService,
  ProviderManagementService 
} from "@/gen/hodei_connect";

const transport = createConnectTransport({
  baseUrl: import.meta.env.VITE_API_URL || "http://localhost:50051",
});

export const jobClient = createClient(JobExecutionService, transport);
export const logClient = createClient(LogStreamService, transport);
export const metricsClient = createClient(MetricsService, transport);
export const providerClient = createClient(ProviderManagementService, transport);
```

### Hook de Logs con Streaming

```typescript
// src/hooks/useJobLogs.ts
import { useEffect, useState, useCallback } from 'react';
import { logClient } from '@/lib/connect';

interface LogEntry {
  timestamp: string;
  level: 'INFO' | 'WARN' | 'ERROR';
  message: string;
  sequence: number;
}

export function useJobLogs(jobId: string) {
  const [logs, setLogs] = useState<LogEntry[]>([]);
  const [isStreaming, setIsStreaming] = useState(false);
  const [isPaused, setIsPaused] = useState(false);

  useEffect(() => {
    if (!jobId || isPaused) return;

    const abortController = new AbortController();
    setIsStreaming(true);

    (async () => {
      try {
        for await (const log of logClient.subscribeLogs(
          { jobId: { value: jobId }, includeHistory: true },
          { signal: abortController.signal }
        )) {
          setLogs(prev => [...prev, {
            timestamp: log.timestamp?.toDate().toISOString() ?? '',
            level: log.isStderr ? 'ERROR' : 'INFO',
            message: log.line,
            sequence: Number(log.sequence),
          }]);
        }
      } catch (err: unknown) {
        if (err instanceof Error && err.name !== 'AbortError') {
          console.error('Log streaming error:', err);
        }
      } finally {
        setIsStreaming(false);
      }
    })();

    return () => abortController.abort();
  }, [jobId, isPaused]);

  const pause = useCallback(() => setIsPaused(true), []);
  const resume = useCallback(() => setIsPaused(false), []);
  const clear = useCallback(() => setLogs([]), []);

  return { logs, isStreaming, isPaused, pause, resume, clear };
}
```

### Test Setup

```typescript
// src/__tests__/setup.ts
import '@testing-library/jest-dom/vitest';
import { cleanup } from '@testing-library/react';
import { afterEach, vi } from 'vitest';

afterEach(() => {
  cleanup();
});

// Mock Connect clients
vi.mock('@/lib/connect', () => ({
  jobClient: {
    queueJob: vi.fn(),
    getJob: vi.fn(),
    cancelJob: vi.fn(),
  },
  logClient: {
    subscribeLogs: vi.fn(),
    getLogs: vi.fn(),
  },
  metricsClient: {
    getSystemMetrics: vi.fn(),
    getJobExecutionMetrics: vi.fn(),
  },
  providerClient: {
    listProviders: vi.fn(),
    getProvider: vi.fn(),
    registerProvider: vi.fn(),
  },
}));
```

---

## 📊 Métricas de Éxito

| Métrica | Objetivo |
|---------|----------|
| Time to First Byte | < 200ms |
| Lighthouse Performance | > 90 |
| Bundle Size | < 500KB gzipped |
| Log Latency | < 100ms desde worker |
| Test Coverage | > 70% |
| Bun Test Speed | < 5s para suite completa |

---

## 🔗 Dependencias

- **Runtime:** Bun 1.1+ para JS/TS
- **Backend:** tonic-web para gRPC-Web support
- **Proto:** Archivos .proto existentes en `/proto`
- **Buf CLI:** Para generación de código TypeScript
- **Diseños:** `/web-stich/` (9 pantallas HTML/Tailwind)

---

## 📚 Referencias

- [Connect Documentation](https://connectrpc.com/docs/introduction)
- [TanStack Query](https://tanstack.com/query/latest)
- [Tailwind CSS 4.x](https://tailwindcss.com/docs)
- [Vite](https://vitejs.dev/)
- [Bun](https://bun.sh/)
- [tonic-web](https://docs.rs/tonic-web/latest/tonic_web/)
- [Testing Library](https://testing-library.com/docs/react-testing-library/intro/)
- [Vitest](https://vitest.dev/)

---

## ✅ Definition of Done

- [ ] Código implementado y revisado
- [ ] Tests unitarios pasando (`bun test`)
- [ ] Coverage > 70%
- [ ] Sin errores de TypeScript (`bun run typecheck`)
- [ ] Lint pasando (`bun run lint`)
- [ ] Lighthouse score > 90
- [ ] Responsive en mobile/tablet/desktop
- [ ] Accesibilidad básica (a11y)
- [ ] Diseño fiel a mockups de web-stich/
- [ ] Integrado en CI/CD
