# EPIC-8: Firecracker Provider

## Resumen

Implementar un provider de Firecracker que permita a Hodei provisionar workers como microVMs, proporcionando aislamiento a nivel de hardware (KVM) con tiempos de arranque sub-segundo para workloads que requieren máximo aislamiento de seguridad.

---

## Objetivos

1. Provisionar workers como microVMs usando Firecracker
2. Proporcionar aislamiento de seguridad a nivel de hardware
3. Mantener tiempos de arranque sub-segundo
4. Integrar con Jailer para sandboxing en producción
5. Mantener compatibilidad con el trait `WorkerProvider` existente

---

## Dependencias

- EPIC-6 completado (Control Loop funcional)
- Host Linux con KVM habilitado para testing
- Firecracker binaries instalados
- Documentación técnica: `docs/providers/firecracker-provider-design.md`

---

## Historias de Usuario

### HU-8.1: Configuración del Firecracker Provider

**Como** operador de plataforma  
**quiero** configurar el Firecracker Provider con paths y recursos  
**para** que Hodei pueda crear microVMs en el host

**Criterios de aceptación:**

- [ ] Crear `FirecrackerConfig` struct con campos:
  - `firecracker_path` (default: "/usr/bin/firecracker")
  - `jailer_path` (default: "/usr/bin/jailer")
  - `data_dir` (default: "/var/lib/hodei/firecracker")
  - `kernel_path` - Path al kernel Linux
  - `rootfs_path` - Path al rootfs base
  - `use_jailer` (default: true)
  - `boot_timeout_secs` (default: 30)
- [ ] Crear `FirecrackerNetworkConfig` para configuración de red
- [ ] Crear `MicroVMResources` para recursos por defecto
- [ ] Implementar `FirecrackerConfigBuilder` con Builder Pattern
- [ ] Cargar configuración desde variables de entorno
- [ ] Validar que binarios y paths existen
- [ ] Tests unitarios para configuración

**Estimación:** 5 puntos

#### Checks de validación (HU-8.1)

- [x] `FirecrackerConfig` struct con todos los campos requeridos
- [x] `FirecrackerNetworkConfig` para configuración de red
- [x] `MicroVMResources` para recursos por defecto (con clamping)
- [x] `FirecrackerConfigBuilder` con Builder Pattern
- [x] Carga de configuración desde variables de entorno (`from_env()`)
- [x] Validación de configuración (paths no vacíos)
- [x] Tests unitarios: `test_firecracker_config_default`, `test_firecracker_config_builder`, `test_firecracker_config_builder_validation`, `test_network_config_builder`, `test_microvm_resources_builder`, `test_microvm_resources_clamping`

---

### HU-8.2: Verificación de Requisitos del Sistema

**Como** sistema  
**quiero** verificar que el host cumple los requisitos para Firecracker  
**para** fallar rápido si no es posible crear microVMs

**Criterios de aceptación:**

- [ ] Verificar acceso a `/dev/kvm` (lectura y escritura)
- [ ] Verificar que Firecracker binary existe y es ejecutable
- [ ] Verificar que Jailer binary existe (si `use_jailer=true`)
- [ ] Verificar que kernel y rootfs existen
- [ ] Verificar permisos de directorio de datos
- [ ] Implementar `health_check()` con estas verificaciones
- [ ] Retornar errores descriptivos si falla alguna verificación
- [ ] Tests unitarios con mocks de filesystem

**Estimación:** 3 puntos

#### Checks de validación (HU-8.2)

- [x] `verify_requirements()` implementado
- [x] Verificación de `/dev/kvm` (existencia y permisos)
- [x] Verificación de Firecracker binary
- [x] Verificación de Jailer binary (si habilitado)
- [x] Verificación de kernel y rootfs
- [x] `health_check()` con verificaciones completas
- [x] Errores descriptivos para cada verificación fallida

---

### HU-8.3: Gestión de Pool de IPs

**Como** sistema  
**quiero** gestionar un pool de direcciones IP para microVMs  
**para** asignar IPs únicas a cada VM

**Criterios de aceptación:**

- [ ] Crear `IpPool` struct para gestión de IPs
- [ ] Configurar subnet desde `FirecrackerNetworkConfig`
- [ ] Implementar `allocate() -> Result<IpAddr>`
- [ ] Implementar `release(ip: IpAddr)`
- [ ] Persistir estado del pool para recuperación tras restart
- [ ] Manejar agotamiento de IPs con error descriptivo
- [ ] Tests unitarios para allocate/release

**Estimación:** 3 puntos

#### Checks de validación (HU-8.3)

- [x] `IpPool` struct implementado
- [x] `from_cidr()` para parsear subnet
- [x] `allocate() -> Result<IpAddr>` implementado
- [x] `release(ip: IpAddr)` implementado
- [x] `is_allocated()` para verificar estado
- [x] Manejo de agotamiento de IPs con error descriptivo
- [x] Tests unitarios: `test_ip_pool_from_cidr`, `test_ip_pool_allocate`, `test_ip_pool_release`, `test_ip_pool_allocation_and_release`

---

### HU-8.4: Configuración de Red (TAP Devices)

**Como** sistema  
**quiero** crear y configurar TAP devices para microVMs  
**para** proporcionar conectividad de red a los workers

**Criterios de aceptación:**

- [ ] Crear `NetworkManager` para gestión de TAP devices
- [ ] Implementar `create_tap_device(vm_id: &str) -> Result<TapDevice>`
- [ ] Configurar IP del host en el TAP device
- [ ] Implementar `destroy_tap_device(tap: &TapDevice)`
- [ ] Configurar NAT/masquerading si `enable_nat=true`
- [ ] Manejar cleanup de TAP devices huérfanos al iniciar
- [ ] Tests de integración (requieren permisos de red)

**Estimación:** 5 puntos

---

### HU-8.5: Creación de microVMs

**Como** scheduler  
**quiero** crear microVMs de worker con Firecracker  
**para** ejecutar jobs con aislamiento de hardware

**Criterios de aceptación:**

- [ ] Implementar `create_worker(&self, spec: &WorkerSpec) -> Result<WorkerHandle>`
- [ ] Preparar directorio de VM con estructura:
  ```
  /var/lib/hodei/firecracker/{vm_id}/
  ├── firecracker.socket
  ├── rootfs.ext4 (copy-on-write o link)
  └── console.log
  ```
- [ ] Iniciar proceso Firecracker (o Jailer si habilitado)
- [ ] Configurar VM via API socket:
  - Boot source (kernel + boot args)
  - Root drive (rootfs)
  - Machine config (vCPUs, memoria)
  - Network interface (TAP device)
- [ ] Inyectar variables via kernel boot args:
  - `HODEI_WORKER_ID`
  - `HODEI_SERVER_ADDRESS`
  - `HODEI_OTP_TOKEN`
  - `HODEI_VM_IP`
  - `HODEI_GATEWAY`
- [ ] Iniciar VM con `InstanceStart` action
- [ ] Esperar boot con timeout configurable
- [ ] Retornar `WorkerHandle` con `provider_resource_id` = vm_id
- [ ] Tests de integración (requieren KVM)

**Estimación:** 13 puntos

---

### HU-8.6: Integración con Jailer

**Como** operador  
**quiero** que las microVMs se ejecuten dentro de Jailer  
**para** tener sandboxing adicional en producción

**Criterios de aceptación:**

- [ ] Implementar `JailerConfig` struct
- [ ] Generar comando de Jailer con parámetros:
  - `--id {vm_id}`
  - `--exec-file {firecracker_path}`
  - `--uid {uid}` / `--gid {gid}`
  - `--chroot-base-dir {data_dir}`
  - `--daemonize`
- [ ] Preparar estructura de directorios para chroot
- [ ] Copiar/linkar binarios y recursos necesarios
- [ ] Manejar paths relativos dentro del jail
- [ ] Tests de integración con Jailer

**Estimación:** 8 puntos

---

### HU-8.7: Monitoreo de Estado de microVMs

**Como** sistema  
**quiero** monitorear el estado de las microVMs  
**para** actualizar el estado en el WorkerRegistry

**Criterios de aceptación:**

- [ ] Implementar `get_worker_status(&self, handle: &WorkerHandle) -> Result<WorkerState>`
- [ ] Verificar estado del proceso Firecracker
- [ ] Consultar estado via API socket si proceso activo
- [ ] Mapear estados:
  - Proceso no existe → Terminated
  - VM Running → Ready/Busy
  - VM Paused → Draining
  - VM Halted → Terminated
- [ ] Detectar crashes y reportar
- [ ] Tests unitarios para mapeo de estados

**Estimación:** 3 puntos

---

### HU-8.8: Destrucción de microVMs

**Como** sistema  
**quiero** destruir microVMs cuando ya no son necesarias  
**para** liberar recursos del host

**Criterios de aceptación:**

- [ ] Implementar `destroy_worker(&self, handle: &WorkerHandle) -> Result<()>`
- [ ] Enviar `SendCtrlAltDel` action para shutdown graceful
- [ ] Esperar terminación con timeout
- [ ] Forzar kill del proceso si no termina
- [ ] Limpiar TAP device
- [ ] Liberar IP al pool
- [ ] Eliminar directorio de VM
- [ ] Manejar VM already destroyed (idempotente)
- [ ] Tests de integración

**Estimación:** 5 puntos

---

### HU-8.9: Obtención de Logs de microVMs

**Como** operador  
**quiero** obtener logs de las microVMs  
**para** diagnosticar problemas

**Criterios de aceptación:**

- [ ] Implementar `get_worker_logs(&self, handle: &WorkerHandle, tail: Option<u32>) -> Result<Vec<LogEntry>>`
- [ ] Leer logs de serial console (`console.log`)
- [ ] Soportar `tail` para limitar líneas
- [ ] Parsear timestamps si disponibles
- [ ] Incluir logs de Firecracker (`firecracker.log`)
- [ ] Manejar archivos de log no existentes
- [ ] Tests unitarios

**Estimación:** 3 puntos

---

### HU-8.10: Capacidades del Provider

**Como** scheduler  
**quiero** conocer las capacidades del Firecracker Provider  
**para** tomar decisiones de scheduling informadas

**Criterios de aceptación:**

- [ ] Implementar `capabilities()` retornando `ProviderCapabilities`:
  - `max_resources`: basado en recursos del host
  - `gpu_support`: false (Firecracker no soporta GPU)
  - `architectures`: detectar arquitectura del host (x86_64/aarch64)
  - `regions`: ["local"]
- [ ] Implementar `can_fulfill(&self, requirements: &JobRequirements) -> bool`
- [ ] Rechazar jobs que requieren GPU
- [ ] Implementar `estimated_startup_time()` (~125ms típico)
- [ ] Tests unitarios

**Estimación:** 3 puntos

---

### HU-8.11: Rootfs con Agent Pre-instalado

**Como** operador  
**quiero** un rootfs base con el agent de Hodei pre-instalado  
**para** que las microVMs puedan conectar al Control Plane

**Criterios de aceptación:**

- [ ] Crear script `scripts/build-firecracker-rootfs.sh`
- [ ] Rootfs basado en Alpine Linux (mínimo)
- [ ] Incluir hodei-worker binary
- [ ] Configurar init script para arrancar agent
- [ ] Configurar networking desde kernel args
- [ ] Documentar proceso de build
- [ ] Publicar rootfs pre-built (opcional)

**Estimación:** 5 puntos

#### Checks de validación (HU-8.11)

- [x] Script `scripts/firecracker/build-rootfs.sh` creado
- [x] Rootfs basado en Alpine Linux (minirootfs)
- [x] Configuración de init scripts para OpenRC
- [x] Script `hodei-network` para configurar red desde kernel args
- [x] Script `hodei-agent` para arrancar worker agent
- [x] Placeholder para hodei-worker binary
- [x] Script `scripts/firecracker/check-env.sh` para verificar entorno
- [x] README con documentación de setup

---

### HU-8.12: Wiring en Server

**Como** operador  
**quiero** habilitar el Firecracker Provider en el servidor  
**para** usar microVMs para provisioning

**Criterios de aceptación:**

- [ ] Añadir `FirecrackerProvider` a `server.rs`
- [ ] Configurar via variables de entorno:
  - `HODEI_FC_ENABLED=1`
  - `HODEI_FC_FIRECRACKER_PATH`
  - `HODEI_FC_KERNEL_PATH`
  - `HODEI_FC_ROOTFS_PATH`
  - `HODEI_FC_USE_JAILER`
- [ ] Registrar provider en `DefaultWorkerProvisioningService`
- [ ] Log de inicialización indicando estado del provider
- [ ] Documentar configuración en README

**Estimación:** 3 puntos

---

### HU-8.13: Tests de Integración

**Como** desarrollador  
**quiero** tests de integración automatizados  
**para** validar el Firecracker Provider

**Criterios de aceptación:**

- [ ] Script de verificación de entorno `scripts/check-firecracker-env.sh`
- [ ] Tests de integración:
  - Crear microVM
  - Verificar estado
  - Obtener logs
  - Destruir microVM
- [ ] Tests marcados con `#[ignore]` para CI sin KVM
- [ ] Documentar cómo ejecutar tests localmente
- [ ] CI job opcional para hosts con KVM

**Estimación:** 5 puntos

---

## Criterios de Aceptación de la Épica

- [x] Todos los tests unitarios pasan (12 tests en firecracker.rs)
- [x] Tests de integración creados (5 tests con `#[ignore]`)
- [x] Documentación actualizada (design doc, EPIC)
- [x] No warnings de compilación
- [x] Provider funcional con trait `WorkerProvider`
- [ ] Métricas Prometheus exportadas (pendiente)
- [x] Rootfs builder script disponible (HU-8.11)

### Checks de validación adicionales

#### HU-8.4: Configuración de Red (TAP Devices)
- [x] `create_tap_device()` implementado usando comandos `ip`
- [x] Configuración de IP en TAP device
- [x] `destroy_tap_device()` implementado

#### HU-8.5: Creación de microVMs
- [x] `create_worker()` implementado
- [x] Preparación de directorio de VM (`prepare_vm_directory()`)
- [x] Inicio de proceso Firecracker (`start_microvm()`)
- [x] Configuración via API socket (`configure_vm_api()`)
- [x] Boot args con variables de entorno
- [x] `WorkerHandle` con metadata (ip_address, tap_device, pid)

#### HU-8.6: Integración con Jailer
- [x] `build_jailer_command()` implementado
- [x] Parámetros: --id, --exec-file, --uid, --gid, --chroot-base-dir
- [x] Configurable via `use_jailer` flag

#### HU-8.7: Monitoreo de Estado de microVMs
- [x] `get_worker_status()` implementado
- [x] `MicroVMState` enum con estados
- [x] `map_vm_state()` para conversión a `WorkerState`
- [x] Test: `test_map_vm_state`

#### HU-8.8: Destrucción de microVMs
- [x] `destroy_worker()` implementado
- [x] Shutdown graceful via API (`SendCtrlAltDel`)
- [x] Force kill si no termina
- [x] Cleanup de TAP device
- [x] Liberación de IP al pool
- [x] Eliminación de directorio de VM
- [x] Idempotente para VMs no existentes

#### HU-8.9: Obtención de Logs de microVMs
- [x] `get_worker_logs()` implementado
- [x] Lectura de console.log
- [x] Soporte `tail` para limitar líneas

#### HU-8.10: Capacidades del Provider
- [x] `capabilities()` implementado
- [x] `gpu_support: false`
- [x] `estimated_startup_time()` = 125ms
- [x] Test: `test_default_capabilities`

#### HU-8.12: Wiring en Server
- [x] `FirecrackerProvider` importado en `server.rs`
- [x] Configuración via `HODEI_FC_ENABLED=1`
- [x] Provider registrado en `DefaultWorkerProvisioningService`
- [x] Logging de inicialización

#### HU-8.13: Tests de Integración
- [x] Tests creados en `crates/infrastructure/tests/firecracker_integration.rs`
- [x] Tests marcados con `#[ignore]`
- [x] Variable `HODEI_FC_TEST=1` para habilitar
- [x] Test de IP pool sin KVM

### Resumen de Implementación

**Archivos creados:**
- `crates/infrastructure/src/providers/firecracker.rs` - Provider completo (~1000 líneas)
- `crates/infrastructure/tests/firecracker_integration.rs` - Tests de integración
- `scripts/firecracker/build-rootfs.sh` - Script para construir rootfs con agent
- `scripts/firecracker/check-env.sh` - Script para verificar entorno
- `scripts/firecracker/README.md` - Documentación de scripts

**Archivos modificados:**
- `Cargo.toml` - Añadidas dependencias (hyper-util, hyperlocal, nix)
- `crates/infrastructure/Cargo.toml` - Dependencias del crate
- `crates/infrastructure/src/providers/mod.rs` - Export del módulo
- `crates/grpc/src/bin/server.rs` - Wiring del provider

**Tests unitarios:**
```
cargo test -p hodei-jobs-infrastructure providers::firecracker
```
- `test_firecracker_config_default`
- `test_firecracker_config_builder`
- `test_firecracker_config_builder_validation`
- `test_network_config_builder`
- `test_microvm_resources_builder`
- `test_microvm_resources_clamping`
- `test_ip_pool_from_cidr`
- `test_ip_pool_allocate`
- `test_ip_pool_release`
- `test_vm_id_generation`
- `test_default_capabilities`
- `test_map_vm_state`

**Variables de entorno:**
- `HODEI_FC_ENABLED=1` - Habilitar provider
- `HODEI_FC_FIRECRACKER_PATH` - Path a Firecracker
- `HODEI_FC_JAILER_PATH` - Path a Jailer
- `HODEI_FC_DATA_DIR` - Directorio de datos
- `HODEI_FC_KERNEL_PATH` - Path al kernel
- `HODEI_FC_ROOTFS_PATH` - Path al rootfs
- `HODEI_FC_USE_JAILER` - Usar Jailer (true/false)

---

## Riesgos y Mitigaciones

| Riesgo | Probabilidad | Impacto | Mitigación |
|--------|--------------|---------|------------|
| Host sin KVM disponible | Alta | Alto | Documentar requisitos, fallback a Docker |
| Complejidad de networking | Media | Alto | Documentación detallada, scripts de setup |
| Permisos insuficientes | Alta | Medio | Documentar requisitos de capabilities |
| Rootfs corrupto | Baja | Alto | Verificación de integridad, backups |

---

## Estimación Total

| Historia | Puntos |
|----------|--------|
| HU-8.1 | 5 |
| HU-8.2 | 3 |
| HU-8.3 | 3 |
| HU-8.4 | 5 |
| HU-8.5 | 13 |
| HU-8.6 | 8 |
| HU-8.7 | 3 |
| HU-8.8 | 5 |
| HU-8.9 | 3 |
| HU-8.10 | 3 |
| HU-8.11 | 5 |
| HU-8.12 | 3 |
| HU-8.13 | 5 |
| **Total** | **64** |

---

## Orden de Implementación Sugerido

```
Fase 1: Fundación (HU-8.1, HU-8.2, HU-8.3)
    │
    ▼
Fase 2: Networking (HU-8.4)
    │
    ▼
Fase 3: Core (HU-8.5, HU-8.7, HU-8.8)
    │
    ▼
Fase 4: Producción (HU-8.6, HU-8.11)
    │
    ▼
Fase 5: Integración (HU-8.9, HU-8.10, HU-8.12, HU-8.13)
```

---

## Referencias

- [Diseño Técnico](../providers/firecracker-provider-design.md)
- [Firecracker GitHub](https://github.com/firecracker-microvm/firecracker)
- [firepilot Rust SDK](https://docs.rs/firepilot/)
- [PRD-V7.0](../PRD-V7.0.md) - Sección 5.2 Modelo de Proveedores
