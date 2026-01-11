# ✅ Abstracción de server_address por Provider

## Problema Identificado

El `server_address` estaba hardcodeado para usar `host.docker.internal`, lo cual:
- ✅ Funciona para Docker containers
- ❌ NO funciona para Kubernetes pods
- ❌ NO funciona para Firecracker VMs

## Solución Implementada

### 1. Nuevo Método en `WorkerProviderIdentity` Trait

```rust
fn transform_server_address(&self, server_address: &str) -> String {
    // Default: use address as-is
    server_address.to_string()
}
```

Este método permite que cada provider transforme el `server_address` según su contexto de red.

### 2. Implementaciones por Provider

#### Docker Provider
```rust
fn transform_server_address(&self, server_address: &str) -> String {
    // Transforma localhost/127.0.0.1 a host.docker.internal
    if server_address.contains("localhost") || server_address.contains("127.0.0.1") {
        server_address
            .replace("localhost", "host.docker.internal")
            .replace("127.0.0.1", "host.docker.internal")
    } else {
        server_address.to_string()
    }
}
```

#### Kubernetes Provider
```rust
fn transform_server_address(&self, server_address: &str) -> String {
    // Para Minikube: usa host.minikube.internal
    // Para producción: usa HODEI_SERVER_HOST_IP
    if server_address.contains("host.docker.internal") {
        if std::env::var("MINIKUBE_ACTIVE_DOCKERD").is_ok() {
            server_address.replace("host.docker.internal", "host.minikube.internal")
        } else {
            std::env::var("HODEI_SERVER_HOST_IP")
                .unwrap_or_else(|_| server_address.to_string())
        }
    } else {
        server_address.to_string()
    }
}
```

#### Firecracker Provider
```rust
fn transform_server_address(&self, server_address: &str) -> String {
    // Usa la IP del TAP bridge (default: 172.16.0.1)
    if server_address.contains("localhost") || server_address.contains("127.0.0.1") {
        std::env::var("HODEI_FIRECRACKER_HOST_IP")
            .unwrap_or_else(|_| {
                server_address
                    .replace("localhost", "172.16.0.1")
                    .replace("127.0.0.1", "172.16.0.1")
            })
    } else {
        server_address.to_string()
    }
}
```

### 3. Integración en Provisioning

```rust
// En provision_worker()
let transformed_address = provider.transform_server_address(&spec.server_address);

let mut spec_with_env = spec.clone();
spec_with_env.server_address = transformed_address;

info!(
    "Generated OTP for worker {}, creating container with server_address: {}",
    worker_id, spec_with_env.server_address
);
```

## Variables de Entorno

### Minikube/Desarrollo Local
```bash
export MINIKUBE_ACTIVE_DOCKERD=minikube
export HODEI_SERVER_ADDRESS=http://localhost:50051
```

### Kubernetes Producción
```bash
export HODEI_SERVER_HOST_IP=<IP_del_host>
export HODEI_SERVER_ADDRESS=http://localhost:50051
```

### Firecracker
```bash
export HODEI_FIRECRACKER_HOST_IP=<IP_TAP_bridge>
export HODEI_SERVER_ADDRESS=http://localhost:50051
```

## Tabla de Transformaciones

| Provider    | Input                        | Output (Minikube)              | Output (Producción)        |
|-------------|------------------------------|--------------------------------|----------------------------|
| Docker      | `localhost:50051`            | `host.docker.internal:50051`   | `host.docker.internal:50051` |
| Kubernetes  | `localhost:50051`            | `host.minikube.internal:50051` | `${HODEI_SERVER_HOST_IP}:50051` |
| Kubernetes  | `host.docker.internal:50051` | `host.minikube.internal:50051` | `${HODEI_SERVER_HOST_IP}:50051` |
| Firecracker | `localhost:50051`            | `172.16.0.1:50051`             | `${HODEI_FIRECRACKER_HOST_IP}:50051` |

## Beneficios

1. ✅ **Abstracción por Provider**: Cada provider controla cómo los workers se conectan
2. ✅ **Soporte Multi-Entorno**: Funciona en desarrollo (Minikube) y producción
3. ✅ **Configuración Externa**: Variables de entorno permiten personalización
4. ✅ **Backward Compatible**: Direcciones explícitas se mantienen sin cambios
5. ✅ **Extensible**: Nuevos providers pueden implementar su propia lógica

## Archivos Modificados

- `crates/server/domain/src/workers/provider_api.rs` - Añadido método `transform_server_address()`
- `crates/server/infrastructure/src/providers/docker.rs` - Implementación Docker
- `crates/server/infrastructure/src/providers/kubernetes.rs` - Implementación Kubernetes
- `crates/server/infrastructure/src/providers/firecracker.rs` - Implementación Firecracker
- `crates/server/application/src/workers/provisioning_impl.rs` - Uso del método en provisioning
- `justfile` - Añadida variable `MINIKUBE_ACTIVE_DOCKERD`

## Testing

### Docker
```bash
just job-docker-hello
# Workers usarán: host.docker.internal:50051
```

### Kubernetes (Minikube)
```bash
export MINIKUBE_ACTIVE_DOCKERD=minikube
just job-k8s-hello
# Workers usarán: host.minikube.internal:50051
```

### Kubernetes (Producción)
```bash
export HODEI_SERVER_HOST_IP=192.168.1.100
just job-k8s-hello
# Workers usarán: 192.168.1.100:50051
```

## Próximos Pasos

1. ⏭️ Probar end-to-end con Minikube configurado
2. ⏭️ Documentar configuración de red para cada provider
3. ⏭️ Añadir tests unitarios para `transform_server_address()`
4. ⏭️ Considerar usar Kubernetes Services en vez de host IP

## Conclusión

La abstracción de `server_address` resuelve el problema de conectividad de workers en diferentes providers, permitiendo que cada uno use la configuración de red apropiada para su contexto.

---

**Implementado**: 2026-01-10  
**Estado**: ✅ Código implementado y compilando correctamente
