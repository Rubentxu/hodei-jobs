# EPIC-11: Tests E2E con Stack Completo - Firecracker Provider

**Versión**: 1.0  
**Fecha**: 2025-12-14  
**Estado**: Planificado  
**Prioridad**: Baja  
**Dependencias**: EPIC-9 (reutiliza infraestructura común)

## Resumen Ejecutivo

Implementar tests End-to-End (E2E) que prueben el stack completo de Hodei Jobs Platform usando el Firecracker Provider. Los workers se ejecutarán como microVMs con tiempos de arranque sub-segundo.

## Objetivos

1. Validar el flujo E2E completo con Firecracker como provider
2. Verificar el provisioning de workers como microVMs
3. Probar networking entre host y microVMs
4. Validar el ciclo de vida completo de microVMs
5. Demostrar el aislamiento a nivel de VM

## Arquitectura del Test

```
┌─────────────────────────────────────────────────────────────────┐
│                    TEST E2E FIRECRACKER                          │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  ┌──────────────┐     ┌──────────────────────────────────────┐  │
│  │  Testcontainers │   │         gRPC Server (Host)           │  │
│  │  PostgreSQL     │◄──┤  - WorkerAgentService                 │  │
│  │  :5432          │   │  - FirecrackerProvider ✓              │  │
│  └──────────────┘     │  - IP Pool: 172.16.0.0/24             │  │
│                        └──────────────┬───────────────────────┘  │
│                                       │                          │
│                                       │ create_worker()          │
│                                       ▼                          │
│  ┌────────────────────────────────────────────────────────────┐  │
│  │                    MicroVM (Firecracker)                    │  │
│  │  ┌────────────────────────────────────────────────────────┐│  │
│  │  │  Linux Kernel (vmlinux)                                ││  │
│  │  │  ┌──────────────────────────────────────────────────┐  ││  │
│  │  │  │  Rootfs (ext4)                                   │  ││  │
│  │  │  │  ┌────────────────────────────────────────────┐  │  ││  │
│  │  │  │  │  Worker Agent Binary                       │  │  ││  │
│  │  │  │  │  - HODEI_TOKEN (via metadata/cmdline)      │  │  ││  │
│  │  │  │  │  - HODEI_SERVER=172.16.0.1                 │  │  ││  │
│  │  │  │  │  - Ejecuta jobs                            │  │  ││  │
│  │  │  │  └────────────────────────────────────────────┘  │  ││  │
│  │  │  └──────────────────────────────────────────────────┘  ││  │
│  │  └────────────────────────────────────────────────────────┘│  │
│  │  Network: tap0 ←→ bridge ←→ Host                           │  │
│  │  IP: 172.16.0.X (del pool)                                  │  │
│  └────────────────────────────────────────────────────────────┘  │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

## Historias de Usuario

### HU-11.1: Preparación de Kernel y Rootfs para Tests

**Como** desarrollador  
**Quiero** un script que prepare kernel y rootfs con el worker incluido  
**Para** poder ejecutar tests E2E sin configuración manual

**Criterios de Aceptación:**
- [ ] Script descarga kernel Linux compatible
- [ ] Script construye rootfs con worker binary
- [ ] Rootfs incluye dependencias mínimas (glibc, etc.)
- [ ] Artefactos se cachean para reutilización

**Tareas:**
- [ ] Crear `scripts/e2e/prepare-firecracker-assets.sh`
- [ ] Modificar `scripts/firecracker/build-rootfs.sh` para incluir worker
- [ ] Documentar requisitos (KVM, root access)

### HU-11.2: Configuración de Networking para MicroVMs

**Como** desarrollador  
**Quiero** networking configurado entre host y microVMs  
**Para** que los workers puedan conectarse al server

**Criterios de Aceptación:**
- [ ] Bridge network creado en el host
- [ ] TAP devices creados para cada microVM
- [ ] IP pool funcional (172.16.0.0/24)
- [ ] Routing configurado correctamente

**Tareas:**
- [ ] Crear `scripts/e2e/setup-fc-network.sh`
- [ ] Implementar verificación de networking en tests
- [ ] Documentar requisitos de permisos (CAP_NET_ADMIN)

### HU-11.3: Test E2E de Provisioning de MicroVM

**Como** desarrollador  
**Quiero** un test que verifique la creación de microVMs  
**Para** validar el FirecrackerProvider

**Criterios de Aceptación:**
- [ ] Job encolado trigger creación de microVM
- [ ] MicroVM arranca en < 1 segundo
- [ ] Worker dentro de VM se conecta al server
- [ ] IP asignada del pool

**Tareas:**
- [ ] Implementar `test_fc_microvm_provisioning()` en `e2e_firecracker_provider.rs`
- [ ] Verificar proceso Firecracker via procfs
- [ ] Verificar conectividad de red

### HU-11.4: Test E2E de Ejecución de Job en MicroVM

**Como** desarrollador  
**Quiero** un test que ejecute un job completo en una microVM  
**Para** validar el flujo E2E

**Criterios de Aceptación:**
- [ ] Job se ejecuta dentro de la microVM
- [ ] Logs llegan al servidor
- [ ] Job se completa exitosamente
- [ ] MicroVM se puede destruir después

**Tareas:**
- [ ] Implementar `test_fc_full_job_execution()`
- [ ] Verificar logs via LogStreamService
- [ ] Verificar estado final en Postgres

### HU-11.5: Test E2E de Aislamiento de MicroVM

**Como** desarrollador  
**Quiero** un test que verifique el aislamiento a nivel de VM  
**Para** demostrar la seguridad del provider

**Criterios de Aceptación:**
- [ ] MicroVM no puede acceder a filesystem del host
- [ ] MicroVM tiene recursos limitados (CPU, memoria)
- [ ] Procesos en microVM aislados del host

**Tareas:**
- [ ] Implementar `test_fc_isolation()`
- [ ] Verificar que `/proc` dentro de VM es diferente
- [ ] Verificar límites de recursos

### HU-11.6: Test E2E de Cleanup de MicroVMs

**Como** desarrollador  
**Quiero** un test que verifique la destrucción de microVMs  
**Para** evitar VMs huérfanas

**Criterios de Aceptación:**
- [ ] MicroVM se destruye después de destroy_worker
- [ ] Proceso Firecracker terminado
- [ ] TAP device eliminado
- [ ] IP devuelta al pool

**Tareas:**
- [ ] Implementar `test_fc_microvm_cleanup()`
- [ ] Verificar que proceso no existe
- [ ] Verificar que IP está disponible en pool

### HU-11.7: Test E2E de Startup Rápido

**Como** desarrollador  
**Quiero** un test que mida el tiempo de arranque  
**Para** verificar el rendimiento de Firecracker

**Criterios de Aceptación:**
- [ ] Tiempo desde create_worker hasta worker registrado < 2s
- [ ] Métricas de tiempo capturadas
- [ ] Comparación con Docker documentada

**Tareas:**
- [ ] Implementar `test_fc_fast_startup()`
- [ ] Medir tiempo de cada fase
- [ ] Generar reporte de rendimiento

### HU-11.8: Script de Ejecución E2E Firecracker

**Como** desarrollador  
**Quiero** un script que ejecute todos los tests E2E de Firecracker  
**Para** automatizar la validación

**Criterios de Aceptación:**
- [ ] Script verifica prerequisitos (KVM, Firecracker)
- [ ] Prepara kernel y rootfs si no existen
- [ ] Configura networking
- [ ] Ejecuta tests E2E
- [ ] Limpia recursos al finalizar

**Tareas:**
- [ ] Crear `scripts/e2e/run-firecracker-e2e.sh`
- [ ] Documentar requisitos de permisos
- [ ] Documentar en GETTING_STARTED.md

## Requisitos Técnicos

### Requisitos del Sistema

| Requisito | Descripción |
|-----------|-------------|
| **OS** | Linux (x86_64 o aarch64) |
| **KVM** | `/dev/kvm` accesible |
| **Firecracker** | Binary instalado |
| **Permisos** | root o CAP_NET_ADMIN + acceso a /dev/kvm |
| **Kernel** | vmlinux compatible (5.10+) |
| **Rootfs** | ext4 con worker binary |

### Variables de Entorno para Tests

| Variable | Valor | Descripción |
|----------|-------|-------------|
| `HODEI_E2E_FC` | `1` | Habilita tests E2E de Firecracker |
| `HODEI_FC_KERNEL_PATH` | `/var/lib/hodei/vmlinux` | Path al kernel |
| `HODEI_FC_ROOTFS_PATH` | `/var/lib/hodei/rootfs.ext4` | Path al rootfs |
| `HODEI_FC_DATA_DIR` | `/tmp/hodei-fc-e2e` | Directorio de datos |

### Estructura de Archivos

```
crates/grpc/tests/
├── e2e_common.rs                # Helpers compartidos
├── e2e_docker_provider.rs       # Tests E2E Docker
├── e2e_kubernetes_provider.rs   # Tests E2E Kubernetes
├── e2e_firecracker_provider.rs  # Tests E2E Firecracker ← NUEVO

scripts/e2e/
├── run-firecracker-e2e.sh       # Script principal
├── prepare-firecracker-assets.sh # Preparar kernel/rootfs
├── setup-fc-network.sh          # Configurar networking
└── cleanup-fc.sh                # Limpiar recursos

scripts/firecracker/
├── build-rootfs.sh              # Construir rootfs (modificado)
└── download-kernel.sh           # Descargar kernel
```

## Desafíos Específicos de Firecracker

### Permisos de Root

**Problema:** Firecracker requiere acceso a `/dev/kvm` y networking.

**Soluciones:**
1. Ejecutar tests como root
2. Usar jailer con permisos específicos
3. Configurar udev rules para acceso a KVM

### Networking Complejo

**Problema:** Cada microVM necesita TAP device y routing.

**Solución:** 
- Script de setup que crea bridge y configura iptables
- IP pool manejado por el provider
- Cleanup automático de TAP devices

### Rootfs con Worker

**Problema:** El worker binary debe estar dentro del rootfs.

**Solución:**
- Build script que crea rootfs mínimo
- Copia worker binary compilado estáticamente
- Init script que arranca worker al boot

### Tiempo de Preparación

**Problema:** Preparar kernel y rootfs toma tiempo.

**Solución:**
- Cachear artefactos en `/var/lib/hodei/`
- Solo reconstruir si hay cambios
- CI/CD puede pre-cachear

## Estimación

| Historia | Complejidad | Estimación |
|----------|-------------|------------|
| HU-11.1 | Alta | 6h |
| HU-11.2 | Alta | 4h |
| HU-11.3 | Alta | 4h |
| HU-11.4 | Alta | 4h |
| HU-11.5 | Media | 3h |
| HU-11.6 | Media | 2h |
| HU-11.7 | Baja | 2h |
| HU-11.8 | Media | 3h |
| **Total** | | **28h** |

## Dependencias

- **EPIC-9**: Reutiliza `e2e_common.rs` y patrones de test
- **EPIC-8**: FirecrackerProvider debe estar completado ✅
- **Linux**: Solo funciona en Linux con KVM

## Criterios de Éxito

1. Tests pasan en Linux con KVM habilitado
2. Tiempo de arranque de microVM < 1 segundo
3. Cleanup completo sin VMs huérfanas
4. Networking funcional entre host y VMs
5. Documentación clara de setup

## Notas de Implementación

### Rootfs Mínimo

El rootfs debe contener:
- Kernel modules necesarios (si aplica)
- glibc o musl (para el worker)
- Worker binary (`/usr/bin/hodei-worker`)
- Init script (`/init` o systemd)
- Configuración de red (`/etc/network/interfaces`)

### Init Script Ejemplo

```bash
#!/bin/sh
# /init

# Montar filesystems
mount -t proc proc /proc
mount -t sysfs sys /sys
mount -t devtmpfs dev /dev

# Configurar red
ip link set eth0 up
ip addr add $HODEI_VM_IP/24 dev eth0
ip route add default via 172.16.0.1

# Iniciar worker
exec /usr/bin/hodei-worker
```

### Comparación de Tiempos

| Fase | Docker | Kubernetes | Firecracker |
|------|--------|------------|-------------|
| Provisioning | 2-5s | 10-30s | 100-500ms |
| Networking | Automático | Automático | Manual |
| Aislamiento | Container | Container | VM |
| Overhead | Bajo | Medio | Muy bajo |
