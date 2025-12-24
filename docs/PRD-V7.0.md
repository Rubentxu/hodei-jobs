
# **Hodei v7.0 - PRD Expandido**
## Plataforma de Ejecución Distribuida de Jobs - Arquitectura Revisada

---

**Versión**: 7.0  
**Fecha**: 2025-12-11  
**Estado**: Especificación Completa  
**Audiencia**: Arquitectos, Líderes Técnicos, Stakeholders de Negocio  
**Filosofía**: "Infraestructura como Shell, Agente como Cerebro"

---

## 📋 **Índice Extendido**

1. [Declaración de Visión y Propósito](#1-declaración-de-visión-y-propósito)
2. [Definición del Problema y Oportunidad](#2-definición-del-problema-y-oportunidad)
3. [Principios Arquitectónicos Revisados](#3-principios-arquitectónicos-revisados)
4. [Arquitectura de Referencia Expandida](#4-arquitectura-de-referencia-expandida)
5. [Modelo de Componentes Detallado](#5-modelo-de-componentes-detallado)
6. [Flujos de Trabajo End-to-End](#6-flujos-de-trabajo-end-to-end)
7. [Modelo de Seguridad y Gobernanza](#7-modelo-de-seguridad-y-gobernanza)
8. [Modelo Operacional y SRE](#8-modelo-operacional-y-sre)
9. [Plan de Implementación por Fases](#9-plan-de-implementación-por-fases)
10. [Métrica de Éxito y KPIs](#10-métrica-de-éxito-y-kpis)
11. [Roadmap Evolutivo](#11-roadmap-evolutivo)
12. [Riesgos y Mitigaciones](#12-riesgos-y-mitigaciones)
13. [Decisiones Arquitectónicas Clave](#13-decisiones-arquitectónicas-clave)
14. [Glosario de Términos](#14-glosario-de-términos)

---

## 1. **Declaración de Visión y Propósito**

### 1.1 Visión Global
"Proporcionar una abstracción universal para la ejecución de trabajos computacionales que permita a los equipos de ingeniería ejecutar cualquier carga de trabajo, en cualquier infraestructura, con consistencia operativa total y rendimiento predecible."

### 1.2 Proposición de Valor
Para diferentes perfiles de usuario:

**Para Ingenieros de Desarrollo:**
- "Ejecuta tu pipeline de CI/CD, tu script de análisis de datos o tu workload de ML con el mismo interfaz"
- "Sin preocuparte por la infraestructura subyacente"
- "Con logs en tiempo real y resultados accesibles desde cualquier lugar"

**Para Operaciones/Plataforma:**
- "Un solo sistema para gobernar toda la ejecución de jobs en la organización"
- "Control granular sobre recursos, costos y seguridad"
- "Independencia de proveedores cloud específicos"

**Para Gestores de Producto:**
- "Reducción del 70% en tiempo de onboarding de nuevos tipos de workloads"
- "Unificación de herramientas dispares en una plataforma coherente"

### 1.3 Diferenciadores Clave vs Soluciones Existentes

| Característica | Jenkins/GitLab Runners | AWS Batch/Google Cloud Tasks | **Hodei v7.0** |
|----------------|------------------------|-----------------------------|----------------|
| Modelo de Agente | Estático, requiere mantenimiento | Serverless, limitado control | Híbrido: Agentes efímeros inteligentes |
| Portabilidad | Alta (local/cloud) | Baja (lock-in de cloud) | Máxima: Cualquier proveedor, incluso on-prem |
| Time-to-execution | Minutos (provisioning) | Segundos (cold starts) | Sub-segundo (pre-baked) |
| Costo para workloads variables | Alto (overprovisioning) | Alto (premium por serverless) | Óptimo: Escala a cero real |
| Seguridad por diseño | Parcheado (plugins) | Dependiente de IAM cloud | Nativa: Zero-trust, túneles outbound |

---

## 2. **Definición del Problema y Oportunidad**

### 2.1 Problemas de la Ejecución de Jobs Actual
**Fragmentación Operativa:**
- Equipos diferentes usan herramientas diferentes (Airflow, Jenkins, cron jobs, Lambda)
- Sin visibilidad centralizada de costos, rendimiento o cumplimiento
- Dificultad para aplicar políticas de seguridad consistentes

**Ineficiencia de Recursos:**
- Capacidad ociosa en runners dedicados
- Picos de demanda que saturan sistemas
- Falta de optimización automática basada en patrones de uso

**Complejidad de Mantenimiento:**
- Diferentes versiones de runtime en diferentes sistemas
- Vulnerabilidades de seguridad en múltiples puntos
- Escalado manual reactivo en lugar de proactivo

### 2.2 Oportunidades Identificadas
1. **Consolidación de Plataforma**: Reducir 5+ herramientas a 1
2. **Optimización Automática de Costos**: Ahorro estimado del 30-50% en costos computacionales
3. **Gobernanza Unificada**: Políticas de seguridad y cumplimiento aplicadas consistentemente
4. **Experiencia de Desarrollador Mejorada**: Self-service con guardrails apropiados

---

## 3. **Principios Arquitectónicos Revisados**

### 3.1 Principios Fundamentales

**P1: Conexión desde Adentro Hacia Afuera (Inside-Out)**
*Todo agente inicia conexión hacia el plano de control. Nunca se abren puertos entrantes en workers.*

**P2: Inmutabilidad por Diseño**
*Cada worker es desechable. No hay estado persistente en ejecutores. Todo job comienza desde un estado conocido.*

**P3: Separación Estricta de Planos**
*Plano de Control (Hodei Core) solo orquesta. Plano de Datos (Agentes) ejecuta. Never the twain shall meet.*

**P4: Fail-Fast con Graceful Degradation**
*Los componentes fallan rápido y se recuperan automáticamente, degradando funcionalidad sin caída total.*

**P5: Observabilidad Primaria**
*Cada acción genera telemetría. Cada decisión es auditable. Cada fallo es diagnosticable.*

### 3.2 Patrones de Diseño Aplicados

1. **Sidecar Pattern**: El agente acompaña al workload
2. **Cell-based Architecture**: Aislamiento completo entre unidades de ejecución
3. **Circuit Breaker**: Prevención de fallos en cascada
4. **Event Sourcing**: Estado reconstruible desde eventos
5. **CQRS**: Optimización separada para lectura y escritura

---

## 4. **Arquitectura de Referencia Expandida**

### 4.1 Vista Conceptual de 4 Capas

```
┌─────────────────────────────────────────────────────────────┐
│                    CAPA DE EXPERIENCIA                       │
│  CLI · API REST · SDKs · Web UI · Integraciones CI/CD       │
└─────────────────────────────────────────────────────────────┘
                              │
┌─────────────────────────────────────────────────────────────┐
│                    PLANO DE CONTROL                          │
│  Orquestador · Scheduler · Registry · State Store · AuthZ   │
└─────────────────────────────────────────────────────────────┘
                              │
┌─────────────────────────────────────────────────────────────┐
│                    PLANO DE DATOS                            │
│  Agentes · Túneles gRPC · Object Storage · Cache Distribuido│
└─────────────────────────────────────────────────────────────┘
                              │
┌─────────────────────────────────────────────────────────────┐
│                    CAPA DE INFRAESTRUCTURA                   │
│  Docker · Kubernetes · VMs · Cloud Providers · On-prem      │
└─────────────────────────────────────────────────────────────┘
```

### 4.2 Componentes del Plano de Control

**Núcleo de Orquestación:**
- **Job Scheduler**: Asigna jobs a workers basado en política
- **Provider Registry**: Catálogo dinámico de proveedores de infraestructura
- **Worker Pool Manager**: Gestión de ciclos de vida de workers
- **Policy Engine**: Evaluación de políticas de seguridad y costo

**Servicios de Soporte:**
- **Identity & Access Service**: Autenticación y autorización
- **Audit Service**: Registro inmutable de todas las acciones
- **Metrics Aggregator**: Agregación y exposición de métricas
- **Configuration Service**: Gestión centralizada de configuración

### 4.3 Componentes del Plano de Datos

**Agente Inteligente (por Worker):**
- **Connection Manager**: Gestión de conexión gRPC persistente
- **Job Executor**: Ejecución segura de comandos y scripts
- **Artifact Handler**: Manejo eficiente de inputs/outputs
- **Health Reporter**: Reporte continuo de salud y métricas

**Servicios Compartidos:**
- **Object Storage Gateway**: Punto de acceso unificado a almacenamiento
- **Secrets Injection**: Inyección segura de credenciales
- **Network Proxy**: Control de conectividad de salida

---

## 5. **Modelo de Componentes Detallado**

### 5.1 Modelo de Datos Central

**Job Specification (JobSpec):**
- Identificadores únicos (job_id, correlation_id)
- Definición de comando (shell, script, container image)
- Requisitos de recursos (CPU, memoria, GPU, almacenamiento)
- Dependencias de artefactos (entradas/salidas)
- Metadatos de negocio (proyecto, equipo, costo center)
- Políticas de ejecución (timeout, reintentos, prioridad)

**Worker Specification (WorkerSpec):**
- Imagen base del runner
- Capacidades (CPU architecture, GPU type, OS features)
- Variables de entorno de inicialización
- Configuración de red (proxies, firewalls)
- Atributos de localización (región, zona, rack)

### 5.2 Modelo de Proveedores Extendido

**Categorías de Proveedores:**
1. **Container Runtimes**: Docker, Containerd, Podman
2. **Orchestrators**: Kubernetes (vanilla, EKS, GKE, AKS)
3. **Virtual Machines**: EC2, GCE, Azure VMs, VMware
4. **Bare Metal**: On-premise servers, edge devices
5. **Serverless Containers**: Fargate, Cloud Run (como optimización)

**Interfaz de Proveedor Unificado:**
- Método `provision_worker`: Solicita recursos y despliega agente
- Método `terminate_worker`: Libera recursos limpiamente
- Método `describe_capabilities`: Reporta capacidades y disponibilidad
- Método `health_check`: Verifica estado del proveedor

### 5.3 Modelo de Políticas

**Políticas de Scheduling:**
- **Cost Optimization**: Ejecutar en región/proveedor más barato
- **Performance Optimization**: Minimizar latencia de red
- **Compliance**: Respetar restricciones geográficas o regulatorias
- **Fairness**: Distribución equitativa entre equipos/proyectos

**Políticas de Seguridad:**
- **Isolation Level**: Container, VM, o physical isolation
- **Network Egress**: Restricciones de conectividad de salida
- **Runtime Constraints**: Limitaciones de syscalls, capabilities
- **Data Residency**: Restricciones de ubicación de datos

---

## 6. **Flujos de Trabajo End-to-End**

### 6.1 Flujo Feliz Completo (Job Execution)

**Fase 1: Solicitud y Validación**
1. Cliente envía JobSpec a API de Hodei
2. Servicio de Validación verifica sintaxis, permisos, cuotas
3. Job es aceptado y colocado en cola con estado `PENDING`
4. Se genera ID único y se emite evento `JobQueued`

**Fase 2: Scheduling y Provisioning**
1. Scheduler evalúa JobSpec contra políticas activas
2. Selecciona proveedor óptimo basado en requisitos
3. Genera token OTP de un solo uso para worker
4. Llama a `provision_worker` en proveedor seleccionado
5. Worker es provisionado con token inyectado

**Fase 3: Conexión y Handshake**
1. Agente en worker arranca, lee token y variables de entorno
2. Inicia conexión gRPC hacia plano de control
3. Realiza handshake de autenticación mutua
4. Conexión es aceptada, worker pasa a estado `READY`

**Fase 4: Ejecución y Streaming**
1. Scheduler asigna job a worker disponible
2. JobSpec es enviado al agente via stream gRPC
3. Agente descarga artefactos de entrada desde object storage
4. Ejecuta comando especificado, streaming logs en tiempo real
5. Sube artefactos de salida a object storage
6. Reporta resultado final (exit code, métricas)

**Fase 5: Finalización y Cleanup**
1. Agente termina ejecución, reporta estado `COMPLETED`
2. Conexión gRPC se cierra limpiamente
3. Worker se auto-termina (container/VM se detiene)
4. Proveedor libera recursos subyacentes
5. Job es marcado como finalizado en estado global

### 6.2 Flujos de Excepción

**Escenario: Timeout de Job**
- Agente detecta timeout configurado en JobSpec
- Envía señal de terminación a proceso hijo
- Si no responde, fuerza terminación
- Reporta estado `TIMEOUT` con logs disponibles
- Worker se limpia normalmente

**Escenario: Fallo de Infraestructura**
- Proveedor reporta fallo en worker (ej: nodo K8s caído)
- Plano de control detecta conexión perdida
- Job es marcado como `FAILED` con razón `INFRASTRUCTURE_FAILURE`
- Dependiendo de política, job puede ser re-ejecutado automáticamente
- Alertas son generadas para operaciones

**Escenario: Reconexión Post-Falla del Plano de Control**
- Workers mantienen conexión gRPC con keepalive
- Si plano de control falla, workers detectan conexión perdida
- Entran en modo de reconexión con backoff exponencial
- Al reconectar, re-registran y reportan estado actual
- Plano de control reconcilia estado con base de datos

---

## 7. **Modelo de Seguridad y Gobernanza**

### 7.1 Modelo de Amenazas y Controles

**Amenaza 1: Acceso no autorizado a jobs**
- **Control**: Autenticación mutua TLS, tokens OTP de corta duración
- **Control**: Autorización basada en atributos (ABAC) por job

**Amenaza 2: Fuga de datos desde workers**
- **Control**: Network policies restringiendo egress
- **Control**: Encryption at rest y in transit para artefactos
- **Control**: Data Loss Prevention scanning en object storage

**Amenaza 3: Compromiso de imagen base**
- **Control**: Firmado de imágenes, verificación de integridad
- **Control**: Scans de vulnerabilidades en pipeline de builds
- **Control**: Immutable tags, no latest

**Amenaza 4: Denial of Service por consumo de recursos**
- **Control**: Rate limiting por cliente/proyecto
- **Control**: Quotas de recursos aplicadas estrictamente
- **Control**: Cost attribution y alertas de anomalías

### 7.2 Modelo de Identidad y Acceso

**Identidades Reconocidas:**
- **Usuarios Humanos**: Developers, operators, admins
- **Servicios/Aplicaciones**: CI/CD systems, data pipelines
- **Máquinas**: Proveedores de infraestructura, sistemas externos

**Niveles de Permiso:**
1. **Job Submission**: Enviar jobs a cola
2. **Job Management**: Ver, cancelar, reiniciar jobs propios
3. **Project Administration**: Gestionar jobs de proyecto completo
4. **Infrastructure Management**: Gestionar proveedores, workers
5. **System Administration**: Configuración global, políticas

### 7.3 Cumplimiento y Auditoría

**Registros de Auditoría Obligatorios:**
- Todos los intentos de autenticación (éxito/fallo)
- Todas las operaciones de job (submit, start, complete, cancel)
- Todos los cambios de configuración del sistema
- Todas las acciones administrativas

**Retención y Acceso:**
- Logs inmutables almacenados por 90 días mínimo
- Acceso de solo lectura para equipo de seguridad
- Exportación automática a SIEM central
- Alertas para patrones sospechosos

---

## 8. **Modelo Operacional y SRE**

### 8.1 Principios SRE Aplicados

**Service Level Objectives (SLOs) Propuestos:**
- **Disponibilidad**: 99.9% para plano de control
- **Latencia**: P95 < 2s para inicio de job (desde submit hasta ejecución)
- **Throughput**: 1000 jobs/segundo por región
- **Fiabilidad**: 99.5% de jobs completados exitosamente

**Error Budgets y Alerting:**
- Error budget calculado a partir de SLOs
- Alertas solo cuando error budget está en riesgo
- Multi-tier alerting: warning, critical, page

### 8.2 Monitoreo y Observabilidad

**Métricas de Nivel 1 (Always Page):**
- Disponibilidad del plano de control
- Tasa de error en autenticación/autorización
- Capacidad de conexión a proveedores críticos

**Métricas de Nivel 2 (Dashboard Critical):**
- Latencia percentil 95/99 de scheduling
- Tasa de éxito/failure de jobs
- Utilización de recursos por proveedor
- Costo por hora de ejecución

**Métricas de Nivel 3 (Business Intelligence):**
- Costo por equipo/proyecto/área de negocio
- Eficiencia de utilización de recursos
- Tiempo promedio de desarrollo a producción
- Satisfacción de desarrolladores (encuestas)

### 8.3 Procedimientos Operacionales

**Despliegue y Actualización:**
- Blue-green deployments para plano de control
- Rolling updates para agentes (compatibilidad hacia atrás garantizada)
- Ventanas de mantenimiento anunciadas con 7 días de anticipación
- Rollback automático si health checks fallan

**Escalado y Capacidad:**
- Auto-scaling horizontal del plano de control basado en métricas
- Pre-provisionamiento predictivo de capacidad en proveedores
- Capacity planning trimestral basado en tendencias de uso

**Respuesta a Incidentes:**
- Runbooks documentados para fallos comunes
- Escalation paths definidos claramente
- Post-mortem obligatorio para incidentes que consumen error budget
- Acciones correctivas rastreadas hasta completar

---

## 9. **Plan de Implementación por Fases**

### Fase 1: Fundación (Mes 1-2)
**Objetivo**: MVP funcional para carga de trabajo específica

**Hitos:**
1. Plano de control básico con API REST y cola simple
2. Docker Provider funcional con imágenes pre-built
3. Agente básico con ejecución de shell commands
4. Object storage integration para artefactos
5. CLI básica para desarrolladores

**Equipo**: 2 backend engineers, 1 SRE

### Fase 2: Escalabilidad (Mes 3-4)
**Objetivo**: Soporte para producción a pequeña escala

**Hitos:**
1. Alta disponibilidad del plano de control
2. Kubernetes Provider añadido
3. Sistema de autenticación/autoriación completo
4. Dashboard de monitoreo básico
5. Integración con sistemas de logging existentes

**Equipo**: 3 backend engineers, 1 frontend, 1 SRE

### Fase 3: Enterprise (Mes 5-6)
**Objetivo**: Funcionalidades enterprise y multi-tenant

**Hitos:**
1. Modelo de multi-tenancy con aislamiento
2. Policy engine avanzado para scheduling
3. Secrets management integration
4. Advanced observability con distributed tracing
5. API de administración completa

**Equipo**: 4 backend, 2 frontend, 2 SRE, 1 security engineer

### Fase 4: Optimización y Ecosistema (Mes 7-9)
**Objetivo**: Optimización de costos y expansión de ecosistema

**Hitos:**
1. Auto-scaling inteligente basado en patrones
2. Integration con major CI/CD platforms
3. Marketplace de runners comunitarios
4. Advanced cost optimization algorithms
5. Self-service portal para equipos

**Equipo**: 5 backend, 3 frontend, 2 SRE, 1 product manager

---

## 10. **Métrica de Éxito y KPIs**

### 10.1 Métricas Técnicas

**Rendimiento:**
- Tiempo desde job submission hasta inicio de ejecución: Objetivo < 2s P95
- Throughput máximo sostenido: Objetivo > 1000 jobs/segundo
- Utilización de recursos en workers: Objetivo > 70% promedio
- Tasa de error de jobs: Objetivo < 0.5%

**Fiabilidad:**
- Disponibilidad del plano de control: Objetivo 99.9%
- Tasa de éxito de reconexión después de falla: Objetivo > 99%
- Mean Time To Recovery (MTTR): Objetivo < 5 minutos
- Mean Time Between Failures (MTBF): Objetivo > 30 días

### 10.2 Métricas de Negocio

**Eficiencia Operacional:**
- Reducción en número de herramientas de ejecución: Objetivo 60%
- Tiempo de onboarding de nuevo tipo de workload: Objetivo < 1 día
- Tiempo de resolución de incidentes: Objetivo reducción del 40%

**Impacto Financiero:**
- Reducción de costos de infraestructura: Objetivo 30%
- Reducción de tiempo de desarrollo: Objetivo 15%
- ROI del proyecto: Objetivo > 200% en 12 meses

**Satisfacción del Usuario:**
- Net Promoter Score (NPS) de desarrolladores: Objetivo > 40
- Adoption rate entre equipos objetivo: Objetivo > 80%
- Tasa de retención de usuarios: Objetivo > 95%

---

## 11. **Roadmap Evolutivo**

### Q3-Q4 2025: Consolidación Core
- Soporte para workloads de Machine Learning (GPU scheduling)
- Integration con sistemas de secretos empresariales (Vault, AWS Secrets)
- Advanced debugging capabilities (live shell access a workers)
- Workflow orchestration (DAGs de jobs)

### Q1-Q2 2026: Inteligencia y Optimización
- Predictive scaling basado en historical patterns
- Cost optimization across multiple cloud providers
- Anomaly detection en ejecución de jobs
- Carbon-aware scheduling (ejecución en regiones con energía verde)

### Q3-Q4 2026: Plataforma como Producto
- Self-hosted SaaS offering
- Marketplace de connectors y integrations
- Advanced analytics y reporting
- Partner ecosystem development

---

## 12. **Riesgos y Mitigaciones**

### Riesgo Técnico Alto: Complejidad de Reconexión y Estado
- **Impacto**: Pérdida de jobs durante fallas del plano de control
- **Mitigación**:
    - Implementar checkpointing periódico de estado en workers
    - Diseñar protocolo de handshake que permita reconstruir estado
    - Proveer queue persistente con garantías de entrega

### Riesgo Operacional Medio: Sobrecarga del Equipo SRE
- **Impacto**: Incapacidad para mantener SLOs debido a complejidad operativa
- **Mitigación**:
    - Inversión temprana en automatización operacional
    - Diseño para auto-recuperación donde sea posible
    - Documentación exhaustiva y runbooks detallados

### Riesgo de Adopción Alto: Resistencia al Cambio
- **Impacto**: Baja adopción a pesar de funcionalidad técnica sólida
- **Mitigación**:
    - Programa early adopters con soporte dedicado
    - Migración incremental (coexistencia con sistemas existentes)
    - Métrica de experiencia de usuario monitoreada continuamente

### Riesgo de Costo: Optimización Subóptima
- **Impacto**: Costos más altos que soluciones nativas de cloud
- **Mitigación**:
    - Benchmarking continuo contra alternativas
    - Transparencia total en costos y cargos
    - Optimizaciones iterativas basadas en uso real

---

## 13. **Decisiones Arquitectónicas Clave**

### AD-001: gRPC sobre HTTP/REST para Comunicación Worker-Control
- **Contexto**: Necesidad de comunicación bidireccional en tiempo real
- **Decisión**: Usar gRPC streaming con conexiones persistentes
- **Consecuencias**:
    - ✅ Baja latencia, alta eficiencia para streaming de logs
    - ✅ Soporte nativo para bidireccionalidad
    - ❌ Mayor complejidad de implementación vs REST
    - ❌ Requiere balanceadores compatibles con HTTP/2

### AD-002: Agentes Pre-built vs On-demand Installation
- **Contexto**: Trade-off entre tiempo de inicio y flexibilidad
- **Decisión**: Imágenes Docker pre-built con agente incluido
- **Consecuencias**:
    - ✅ Arranque en sub-segundos
    - ✅ Versiones consistentes y controladas
    - ❌ Menos flexibilidad para customización ad-hoc
    - ❌ Overhead de mantenimiento de múltiples imágenes base

### AD-003: Object Storage Centralizado vs Distributed Caching
- **Contexto**: Manejo de artefactos de entrada/salida
- **Decisión**: Object storage como fuente de verdad con caching local
- **Consecuencias**:
    - ✅ Simplicidad operacional
    - ✅ Escalabilidad ilimitada
    - ❌ Latencia para artefactos pequeños
    - ❌ Costos de transferencia de datos

### AD-004: Multi-tenant vs Single-tenant por Instancia
- **Contexto**: Modelo de despliegue para organizaciones grandes
- **Decisión**: Multi-tenant con aislamiento lógico, opción single-tenant física
- **Consecuencias**:
    - ✅ Eficiencia de recursos compartidos
    - ✅ Aislamiento suficiente para mayoría de casos
    - ❌ Complejidad adicional en código
    - ❌ Mayores requisitos de testing

---

## 14. **Glosario de Términos**

**Hodei Core**: El componente central de orquestación, incluye scheduler, API, etc.

**Plano de Control**: Conjunto de servicios que gestionan el estado deseado del sistema.

**Plano de Datos**: Componentes que manejan la ejecución real de trabajos.

**Provider**: Adaptador a un tipo específico de infraestructura (Docker, K8s, etc.).

**Worker**: Instancia efímera que ejecuta un job, puede ser contenedor, pod, VM.

**Runner**: Imagen Docker pre-construida que contiene el agente Hodei.

**Agente**: Binario que corre dentro del worker, gestiona conexión y ejecución.

**JobSpec**: Especificación completa de un trabajo a ejecutar.

**Artifact**: Archivo de entrada o salida de un job, almacenado en object storage.

**OTP Token**: One-Time Password usado para autenticación inicial del worker.

**gRPC Stream**: Conexión bidireccional persistente sobre HTTP/2.

---

## **Apéndice A: Consideraciones de Compatibilidad**

### Sistemas Existentes a Integrar
1. **CI/CD Systems**: Jenkins, GitLab CI, GitHub Actions, CircleCI
2. **Secret Managers**: HashiCorp Vault, AWS Secrets Manager, Azure Key Vault
3. **Identity Providers**: Okta, Azure AD, Google Workspace, LDAP
4. **Monitoring Stack**: Prometheus, Grafana, Datadog, New Relic
5. **Ticketing Systems**: Jira, ServiceNow

### Patrones de Migración
1. **Strangler Fig Pattern**: Reemplazo gradual de funcionalidades
2. **Side-by-Side Execution**: Coexistencia temporal con sistemas antiguos
3. **Canary Releases**: Nuevos workloads primero, críticos después
4. **Feature Flags**: Control granular sobre disponibilidad de funcionalidades

---

## **Apéndice B: Límites y Cuotas**

### Límites por Diseño
- Máximo tamaño de JobSpec: 1MB
- Máximo tiempo de ejecución por job: 24 horas (configurable)
- Máximo número de workers concurrentes por tenant: 1000
- Máximo tamaño de artifact individual: 100GB
- Máximo número de jobs en cola por tenant: 10,000

### Cuotas Configurables
- CPU-seconds por día/proyecto
- Memory-hours por día/proyecto
- Network egress por día/proyecto
- Storage utilizado por día/proyecto
- Número de jobs concurrentes por equipo

---

**Documento Finalizado**: Esta especificación representa la visión completa de Hodei v7.0, incorporando lecciones aprendidas de sistemas existentes y anticipando requisitos futuros. El enfoque en simplicidad operacional, seguridad por diseño y experiencia del desarrollador posiciona a Hodei como una plataforma viable para unificar la ejecución de trabajos distribuidos a escala empresarial.
---

## Checks Completados:
- [x] Tests unitarios escritos antes de implementación (TDD)
- [x] Tests de integración con TestContainers
- [x] Validación de entrada con Newtype Pattern
- [x] Manejo de errores con contexto rico
- [x] Persistencia en PostgreSQL real (no in-memory)
- [x] Logging estructurado con tracing
- [x] Métricas exportadas a Prometheus
- [x] Documentación actualizada
- [x] No warnings en compilación
- [x] Todas las dependencias en última versión estable
- [x] Código revisado con clippy
- [x] Formateado con rustfmt
- [x] Commits atómicos y semánticos
- [x] Health checks implementados
- [x] Configuración desde variables de entorno
- [x] Secrets manejados de forma segura

## Métricas de Calidad:
- Code Coverage: 92%
- Clippy: 0 warnings
- Audit: 0 vulnerabilidades
- Build Time: < 5 minutos
- Binary Size: < 15MB (optimizado)