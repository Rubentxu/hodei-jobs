# Firecracker Provider - Diseño Técnico

## Resumen Ejecutivo

El **Firecracker Provider** permite a Hodei provisionar workers como microVMs usando Firecracker. Proporciona aislamiento a nivel de hardware (KVM) con tiempos de arranque sub-segundo, ideal para workloads que requieren máximo aislamiento de seguridad.

---

## 1. Arquitectura

### 1.1 Componentes

```
┌─────────────────────────────────────────────────────────────┐
│                    Hodei Control Plane                       │
│  ┌─────────────────┐    ┌─────────────────┐                 │
│  │ SchedulerService│───▶│FirecrackerProvider│               │
│  └─────────────────┘    └────────┬────────┘                 │
│                                  │                           │
└──────────────────────────────────┼───────────────────────────┘
                                   │ Unix Socket API / firepilot
                                   ▼
┌─────────────────────────────────────────────────────────────┐
│                    Host Machine (KVM enabled)                │
│  ┌─────────────────┐    ┌─────────────────┐                 │
│  │   microVM 1     │    │   microVM 2     │                 │
│  │ ┌─────────────┐ │    │ ┌─────────────┐ │                 │
│  │ │ Hodei Agent │ │    │ │ Hodei Agent │ │                 │
│  │ │  (rootfs)   │ │    │ │  (rootfs)   │ │                 │
│  │ └─────────────┘ │    │ └─────────────┘ │                 │
│  │   Firecracker   │    │   Firecracker   │                 │
│  │     VMM         │    │     VMM         │                 │
│  └─────────────────┘    └─────────────────┘                 │
│           │                      │                           │
│           └──────────┬───────────┘                           │
│                      │                                       │
│              ┌───────▼───────┐                               │
│              │   Jailer      │  (sandboxing)                 │
│              └───────────────┘                               │
└─────────────────────────────────────────────────────────────┘
```

### 1.2 Flujo de Provisioning

1. **Scheduler** decide `ProvisionWorker` con `provider_id` de Firecracker
2. **FirecrackerProvider** prepara configuración de microVM
3. Inicia proceso Firecracker con Jailer (sandboxing)
4. Configura kernel, rootfs, networking via API socket
5. Arranca microVM con agent pre-instalado en rootfs
6. Agent conecta al Control Plane via gRPC (a través de TAP interface)
7. Worker se registra y queda disponible para jobs

---

## 2. Dependencias

### 2.1 Crates Rust

```toml
[dependencies]
firepilot = "1.2"
firepilot_models = "1.2"
tokio = { version = "1", features = ["full", "process"] }
hyper = { version = "1", features = ["client"] }
hyperlocal = "0.9"  # Unix socket HTTP client
```

### 2.2 Requisitos del Sistema

| Componente | Versión Mínima | Notas |
|------------|----------------|-------|
| Linux Kernel | 4.14+ | Con KVM habilitado |
| Firecracker | 1.7+ | Binary en PATH o configurado |
| Jailer | 1.7+ | Incluido con Firecracker |
| KVM | - | `/dev/kvm` accesible |

### 2.3 Verificación de Requisitos

```bash
# Verificar KVM
[ -r /dev/kvm ] && [ -w /dev/kvm ] && echo "KVM OK"

# Verificar Firecracker
firecracker --version

# Verificar capacidades
setcap cap_net_admin+ep /path/to/firecracker
```

---

## 3. Modelo de Datos

### 3.1 FirecrackerConfig

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FirecrackerConfig {
    /// Path al binario de Firecracker
    pub firecracker_path: String,
    /// Path al binario de Jailer
    pub jailer_path: String,
    /// Directorio base para sockets y datos de VMs
    pub data_dir: String,
    /// Path al kernel Linux para las microVMs
    pub kernel_path: String,
    /// Path al rootfs base (con agent pre-instalado)
    pub rootfs_path: String,
    /// Configuración de red
    pub network: FirecrackerNetworkConfig,
    /// Recursos por defecto
    pub default_resources: MicroVMResources,
    /// Usar Jailer para sandboxing (recomendado en producción)
    pub use_jailer: bool,
    /// UID/GID para Jailer
    pub jailer_uid: Option<u32>,
    pub jailer_gid: Option<u32>,
    /// Timeout para arranque de VM (segundos)
    pub boot_timeout_secs: u64,
}

impl Default for FirecrackerConfig {
    fn default() -> Self {
        Self {
            firecracker_path: "/usr/bin/firecracker".to_string(),
            jailer_path: "/usr/bin/jailer".to_string(),
            data_dir: "/var/lib/hodei/firecracker".to_string(),
            kernel_path: "/var/lib/hodei/vmlinux".to_string(),
            rootfs_path: "/var/lib/hodei/rootfs.ext4".to_string(),
            network: FirecrackerNetworkConfig::default(),
            default_resources: MicroVMResources::default(),
            use_jailer: true,
            jailer_uid: Some(1000),
            jailer_gid: Some(1000),
            boot_timeout_secs: 30,
        }
    }
}
```

### 3.2 FirecrackerNetworkConfig

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FirecrackerNetworkConfig {
    /// Prefijo para interfaces TAP
    pub tap_prefix: String,
    /// Subnet para microVMs (CIDR)
    pub subnet: String,
    /// Gateway IP
    pub gateway_ip: String,
    /// Interface del host para NAT
    pub host_interface: String,
    /// Habilitar NAT para acceso a internet
    pub enable_nat: bool,
}

impl Default for FirecrackerNetworkConfig {
    fn default() -> Self {
        Self {
            tap_prefix: "fc-tap".to_string(),
            subnet: "172.16.0.0/24".to_string(),
            gateway_ip: "172.16.0.1".to_string(),
            host_interface: "eth0".to_string(),
            enable_nat: true,
        }
    }
}
```

### 3.3 MicroVMResources

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MicroVMResources {
    /// Número de vCPUs (1-32)
    pub vcpu_count: u8,
    /// Memoria en MiB (128-32768)
    pub mem_size_mib: u32,
    /// Tamaño del disco en MiB (opcional, para disco adicional)
    pub disk_size_mib: Option<u32>,
    /// Habilitar hyperthreading
    pub ht_enabled: bool,
}

impl Default for MicroVMResources {
    fn default() -> Self {
        Self {
            vcpu_count: 1,
            mem_size_mib: 512,
            disk_size_mib: None,
            ht_enabled: false,
        }
    }
}
```

---

## 4. Implementación del Trait WorkerProvider

```rust
pub struct FirecrackerProvider {
    provider_id: ProviderId,
    config: FirecrackerConfig,
    capabilities: ProviderCapabilities,
    /// Tracking de VMs activas: worker_id -> MicroVMInstance
    active_vms: Arc<RwLock<HashMap<WorkerId, MicroVMInstance>>>,
    /// Pool de IPs disponibles
    ip_pool: Arc<RwLock<IpPool>>,
}

struct MicroVMInstance {
    worker_id: WorkerId,
    vm_id: String,
    socket_path: String,
    tap_device: String,
    ip_address: String,
    process_handle: Option<Child>,
    state: MicroVMState,
}

#[async_trait]
impl WorkerProvider for FirecrackerProvider {
    fn provider_id(&self) -> &ProviderId { &self.provider_id }
    fn provider_type(&self) -> ProviderType { ProviderType::Firecracker }
    fn capabilities(&self) -> &ProviderCapabilities { &self.capabilities }
    
    async fn create_worker(&self, spec: &WorkerSpec) -> Result<WorkerHandle, ProviderError> {
        // 1. Allocate IP from pool
        // 2. Create TAP device
        // 3. Prepare VM directory structure
        // 4. Copy/link rootfs
        // 5. Start Firecracker process (with Jailer if enabled)
        // 6. Configure VM via API socket
        // 7. Start VM
        // 8. Wait for boot and agent connection
    }
    
    async fn get_worker_status(&self, handle: &WorkerHandle) -> Result<WorkerState, ProviderError> {
        // Query VM state via API socket or process status
    }
    
    async fn destroy_worker(&self, handle: &WorkerHandle) -> Result<(), ProviderError> {
        // 1. Send shutdown to VM
        // 2. Wait for graceful shutdown
        // 3. Force kill if needed
        // 4. Cleanup TAP device
        // 5. Release IP to pool
        // 6. Remove VM directory
    }
    
    async fn get_worker_logs(&self, handle: &WorkerHandle, tail: Option<u32>) -> Result<Vec<LogEntry>, ProviderError> {
        // Read from VM's serial console log file
    }
    
    async fn health_check(&self) -> Result<HealthStatus, ProviderError> {
        // Check KVM availability
        // Check Firecracker binary
        // Check network configuration
    }
}
```

---

## 5. Mapeo de Estados

| MicroVM State | WorkerState   |
|---------------|---------------|
| NotStarted    | Creating      |
| Starting      | Creating      |
| Running       | Ready/Busy    |
| Paused        | Draining      |
| Halted        | Terminated    |

---

## 6. Configuración de Red

### 6.1 Setup de TAP Device

```bash
# Crear TAP device para una VM
ip tuntap add dev fc-tap-{vm_id} mode tap
ip addr add 172.16.0.1/30 dev fc-tap-{vm_id}
ip link set dev fc-tap-{vm_id} up

# Habilitar IP forwarding
echo 1 > /proc/sys/net/ipv4/ip_forward

# NAT para acceso a internet
iptables -t nat -A POSTROUTING -o eth0 -j MASQUERADE
iptables -A FORWARD -m conntrack --ctstate RELATED,ESTABLISHED -j ACCEPT
iptables -A FORWARD -i fc-tap-{vm_id} -o eth0 -j ACCEPT
```

### 6.2 Configuración en la VM

```json
{
  "iface_id": "eth0",
  "guest_mac": "AA:FC:00:00:00:01",
  "host_dev_name": "fc-tap-{vm_id}"
}
```

---

## 7. Rootfs con Agent Pre-instalado

### 7.1 Estructura del Rootfs

```
/
├── bin/
│   └── hodei-worker          # Agent binary
├── etc/
│   ├── hodei/
│   │   └── config.toml       # Configuración base
│   └── init.d/
│       └── hodei-agent       # Init script
├── lib/
│   └── ...                   # Librerías necesarias
└── var/
    └── log/
        └── hodei/            # Logs del agent
```

### 7.2 Script de Inicio

```bash
#!/bin/sh
# /etc/init.d/hodei-agent

# Leer variables de entorno desde kernel cmdline
eval $(cat /proc/cmdline | tr ' ' '\n' | grep HODEI_)

# Configurar red
ip addr add ${HODEI_VM_IP}/30 dev eth0
ip link set eth0 up
ip route add default via ${HODEI_GATEWAY}

# Iniciar agent
exec /bin/hodei-worker \
    --worker-id=${HODEI_WORKER_ID} \
    --server-address=${HODEI_SERVER_ADDRESS} \
    --otp-token=${HODEI_OTP_TOKEN}
```

### 7.3 Kernel Boot Args

```
console=ttyS0 reboot=k panic=1 pci=off \
HODEI_WORKER_ID={worker_id} \
HODEI_SERVER_ADDRESS={server_address} \
HODEI_OTP_TOKEN={otp_token} \
HODEI_VM_IP={vm_ip} \
HODEI_GATEWAY={gateway_ip}
```

---

## 8. Jailer (Sandboxing)

### 8.1 Configuración de Jailer

```rust
struct JailerConfig {
    /// ID único para el jail
    id: String,
    /// UID para el proceso
    uid: u32,
    /// GID para el proceso
    gid: u32,
    /// Directorio chroot
    chroot_base_dir: String,
    /// Netns (opcional)
    netns: Option<String>,
    /// Cgroup (opcional)
    cgroup: Option<String>,
}
```

### 8.2 Comando de Jailer

```bash
jailer \
    --id {vm_id} \
    --exec-file /usr/bin/firecracker \
    --uid {uid} \
    --gid {gid} \
    --chroot-base-dir /srv/jailer \
    --daemonize \
    -- \
    --api-sock /run/firecracker.socket
```

---

## 9. Seguridad

### 9.1 Aislamiento

| Capa | Mecanismo |
|------|-----------|
| CPU/Memory | KVM hardware virtualization |
| Filesystem | Chroot + separate rootfs |
| Network | TAP devices + network namespaces |
| Process | Jailer + seccomp filters |
| Capabilities | Dropped capabilities |

### 9.2 Seccomp

Firecracker aplica filtros seccomp estrictos por defecto:
- Solo syscalls necesarias permitidas
- Filtros específicos por thread

### 9.3 Permisos Requeridos

```bash
# Capacidades necesarias para Firecracker
setcap cap_net_admin+ep /usr/bin/firecracker

# Permisos de KVM
usermod -a -G kvm hodei-user

# Permisos de directorio
chown -R hodei-user:hodei-group /var/lib/hodei/firecracker
chmod 750 /var/lib/hodei/firecracker
```

---

## 10. Configuración de Entorno

### Variables de Entorno del Provider

| Variable | Descripción | Default |
|----------|-------------|---------|
| `HODEI_FC_FIRECRACKER_PATH` | Path al binario | `/usr/bin/firecracker` |
| `HODEI_FC_JAILER_PATH` | Path al jailer | `/usr/bin/jailer` |
| `HODEI_FC_DATA_DIR` | Directorio de datos | `/var/lib/hodei/firecracker` |
| `HODEI_FC_KERNEL_PATH` | Path al kernel | `/var/lib/hodei/vmlinux` |
| `HODEI_FC_ROOTFS_PATH` | Path al rootfs | `/var/lib/hodei/rootfs.ext4` |
| `HODEI_FC_USE_JAILER` | Usar Jailer | `true` |
| `HODEI_FC_SUBNET` | Subnet para VMs | `172.16.0.0/24` |

---

## 11. Métricas y Observabilidad

### Métricas Prometheus

- `hodei_fc_vms_created_total` - Total de microVMs creadas
- `hodei_fc_vms_failed_total` - Total de microVMs fallidas
- `hodei_fc_vm_boot_duration_seconds` - Tiempo de boot
- `hodei_fc_vm_memory_bytes` - Memoria asignada por VM
- `hodei_fc_vm_vcpus` - vCPUs asignadas por VM
- `hodei_fc_api_requests_total` - Requests a API de Firecracker

### Logs

- Serial console: `/var/lib/hodei/firecracker/{vm_id}/console.log`
- Firecracker logs: `/var/lib/hodei/firecracker/{vm_id}/firecracker.log`
- Agent logs: Via gRPC stream al Control Plane

---

## 12. Testing

### 12.1 Unit Tests

- Mock de API socket responses
- Validación de configuración
- IP pool management

### 12.2 Integration Tests

- Requiere host con KVM
- Tests con `#[ignore]` para CI sin KVM
- Cleanup automático de VMs

### 12.3 Test Environment

```bash
# Verificar entorno de test
./scripts/check-firecracker-env.sh

# Ejecutar tests de integración
HODEI_FC_TEST=1 cargo test --features firecracker-integration
```

---

## 13. Limitaciones Conocidas

1. **Solo Linux** - Firecracker requiere KVM (Linux only)
2. **x86_64 y aarch64** - Solo estas arquitecturas soportadas
3. **No GPU** - Firecracker no soporta passthrough de GPU
4. **Networking manual** - Requiere configuración de TAP/NAT
5. **Rootfs pre-built** - Requiere imagen con agent pre-instalado

---

## 14. Comparación con Docker Provider

| Aspecto | Docker | Firecracker |
|---------|--------|-------------|
| Aislamiento | Namespaces/cgroups | Hardware (KVM) |
| Boot time | ~100ms | ~125ms |
| Memory overhead | ~10MB | ~5MB |
| Security | Container escape posible | VM escape muy difícil |
| GPU support | Sí (nvidia-docker) | No |
| Networking | Docker network | TAP + NAT manual |
| Complejidad | Baja | Media-Alta |

---

## 15. Referencias

- [Firecracker GitHub](https://github.com/firecracker-microvm/firecracker)
- [Firecracker Design](https://github.com/firecracker-microvm/firecracker/blob/main/docs/design.md)
- [firepilot Rust SDK](https://docs.rs/firepilot/)
- [Firecracker API Spec](https://github.com/firecracker-microvm/firecracker/blob/main/src/firecracker/swagger/firecracker.yaml)
- [Production Host Setup](https://github.com/firecracker-microvm/firecracker/blob/main/docs/prod-host-setup.md)
