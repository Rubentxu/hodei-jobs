# Gap Analysis: EPIC-12 Web Dashboard

## Estado Actual vs. Especificación

Este documento detalla los gaps y deuda técnica identificados tras analizar la especificación `EPIC-12-WEB-DASHBOARD.md` y la implementación actual en el directorio `web/`.

### 1. Dashboard UI (`web/src/pages/DashboardPage.tsx`)

**Estado:** ❌ Implementación Completa Faltante
La página existe pero es un "mockup estático" migrado de HTML.

*   **Gap Critico:** No utiliza ningún hook (`useJobs`, `useMetrics`) para obtener datos. Todo el contenido está harcodeado.
    *   `StatsCard`: Valores fijos ("1,240", "12", etc.).
    *   `RecentJobCard`: Lista fija de jobs de ejemplo.
    *   `SystemHealth`: Gráfico SVG estático y valores fijos.
*   **Gap UI:** No hay gestión de estados de carga (`isLoading`) ni de error (`isError`).
*   **Gap Interacción:** El botón de "filtro" (search) y logs no son funcionales.

### 2. Integración de Datos (Hooks)

#### `useJobs.ts`
**Estado:** ⚠️ Parcialmente Implementado
*   **Positivo:** Contiene la lógica para conectar con `jobClient` real (gRPC) y un fallback a mocks.
*   **Gap:** No está conectado a `DashboardPage`.
*   **Gap:** La interfaz `Job` definida manualmente podría duplicar tipos generados en `src/gen`.

#### `useMetrics.ts`
**Estado:** ❌ Implementación Faltante
*   **Gap Critico:** Solo devuelve datos mock. No hay lógica para llamar a `metricsClient` (gRPC).
*   **Deuda:** Contiene `TODO: Replace with gRPC call`.

#### `useJobLogs.ts`
**Estado:** ✅ Aparentemente Completo
*   Parece tener la lógica de suscripción (`subscribeLogs`) implementada. (No verificado en UI, pero el código existe).

### 3. Infraestructura General

*   **Clientes gRPC:** Existen y están configurados en `src/lib/connect.ts` correctamente con interceptores.
*   **Tests:** Existen setup files, pero no hay tests unitarios o de integración que verifiquen el renderizado del Dashboard con datos reales.

## Plan de Acción Recomendado

1.  **Conectar UI con Hooks:**
    *   Refactorizar `DashboardPage.tsx` para usar `useSystemMetrics` (o equivalente) para las Stats Cards.
    *   Usar `useJobs` para la lista de "Recent Executions".

2.  **Implementar `useMetrics.ts`:**
    *   Implementar llamadas reales a `metricsClient.getSystemMetrics` y `getJobExecutionMetrics` (o los métodos correspondientes en el proto).

3.  **Habilitar Datos Reales:**
    *   Asegurar que `DashboardPage` maneje los estados de carga/error elegantemente.

4.  **Limpieza:**
    *   Eliminar valores harcodeados en componentes UI.
