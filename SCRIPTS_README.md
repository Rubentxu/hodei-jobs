# Hodei Jobs Platform - Scripts Overview

Este documento describe todos los scripts disponibles en el proyecto y cómo usarlos según la documentación de GETTING_STARTED.md.

## 📋 Scripts Disponibles

### 🚀 start.sh - Inicio Rápido
**Propósito**: Script principal para iniciar toda la plataforma con un solo comando.

**Uso**:
```bash
./scripts/start.sh              # Inicia la plataforma
./scripts/start.sh --build-worker   # Incluye build de worker
./scripts/start.sh --help          # Ver ayuda
```

**Qué hace**:
1. Verifica dependencias (Docker, Docker Compose)
2. Ejecuta cleanup (opcional)
3. Construye imagen del worker (opcional)
4. Crea archivo .env si no existe
5. Inicia todos los servicios con docker-compose.prod.yml
6. Espera a que la API esté lista
7. Muestra estado y URLs de acceso

---

### 🔧 setup.sh - Configuración Inicial
**Propósito**: Configura el entorno de desarrollo completo.

**Uso**:
```bash
./scripts/setup.sh              # Setup completo
./scripts/setup.sh --minimal    # Setup mínimo
./scripts/setup.sh --help       # Ver ayuda
```

**Qué instala**:
- Rust (cargo, rustc)
- Node.js & npm
- Docker & docker-compose
- Herramientas auxiliares (just, bacon, buf)
- Dependencias del proyecto

---

### 🏗️ dev.sh - Entorno de Desarrollo
**Propósito**: Inicia el entorno de desarrollo con hot reload.

**Uso**:
```bash
./scripts/dev.sh                # Full stack
./scripts/dev.sh db            # Solo base de datos
./scripts/dev.sh backend       # Solo backend
./scripts/dev.sh frontend      # Solo frontend
./scripts/dev.sh test          # Ejecutar tests
./scripts/dev.sh clean         # Limpiar todo
```

**Características**:
- PostgreSQL en Docker
- Backend con hot reload (Bacon)
- Frontend con HMR (Vite)

---

### 🎯 run_maven_job.sh - Ejecutor de Job Maven
**Propósito**: Ejecuta el job complejo de verificación Maven según GETTING_STARTED.md.

**Uso**:
```bash
./scripts/run_maven_job.sh         # Ejecutar job
./scripts/run_maven_job.sh --help  # Ver ayuda
```

**Qué hace**:
1. Verifica que la API esté corriendo
2. Lee el script de verificación Maven
3. Envía el job con parámetros correctos:
   - 2 CPUs
   - 4GB memoria
   - 1800s timeout
4. Muestra el Job ID
5. Proporciona comandos para monitoreo

---

### 📊 watch_logs.sh - Monitor de Logs
**Propósito**: Monitorea y guarda logs de jobs en ejecución.

**Uso**:
```bash
./scripts/watch_logs.sh
```

**Características**:
- Detecta jobs en estado RUNNING o ASSIGNED
- Stream de logs en tiempo real
- Guarda logs en `build/logs/<job_id>.log`
- Soporte para stdout y stderr

---

### 🧹 cleanup.sh - Limpieza de Recursos
**Propósito**: Limpia recursos Docker después de pruebas.

**Uso**:
```bash
./scripts/cleanup.sh              # Con confirmación
./scripts/cleanup.sh --force      # Sin confirmación
```

**Qué limpia**:
- Todos los contenedores (stop + remove)
- Imágenes no usadas (mantiene hodei-jobs-*)
- Volúmenes no usados
- Redes no usadas
- Build cache

---

### 🧪 verification/maven_build_job.sh
**Propósito**: Script de payload para el job Maven.

**Uso**: Se ejecuta automáticamente como parte del job.

**Qué hace**:
1. Instala asdf v0.14.0
2. Configura plugins de Java y Maven
3. Instala Java 17 y Maven 3.9.6
4. Clona repositorio de prueba
5. Ejecuta `mvn clean install`
6. Valida la construcción

---

## 🔄 Flujo de Trabajo Recomendado

### Para Producción/Rápido Inicio:
```bash
# 1. Iniciar plataforma
./scripts/start.sh --build-worker

# 2. Ejecutar job de prueba
./scripts/run_maven_job.sh

# 3. Monitorear logs (en otra terminal)
./scripts/watch_logs.sh

# 4. Limpiar cuando termines
./scripts/cleanup.sh
```

### Para Desarrollo:
```bash
# 1. Setup inicial (solo primera vez)
./scripts/setup.sh

# 2. Iniciar entorno desarrollo
./scripts/dev.sh

# 3. En otra terminal, monitorear logs
./scripts/watch_logs.sh

# 4. Ejecutar tests
./scripts/dev.sh test
```

---

## 📝 Verificación de Estado

### Verificar servicios:
```bash
docker compose -f docker-compose.prod.yml ps
```

### Verificar API:
```bash
grpcurl -plaintext localhost:50051 list
```

### Verificar logs de servicios:
```bash
docker compose -f docker-compose.prod.yml logs -f api
```

### Ver jobs en base de datos:
```bash
docker exec -it hodei-jobs-postgres psql -U postgres -d hodei -c "SELECT * FROM jobs LIMIT 5;"
```

---

## 🐛 Troubleshooting

### API no responde:
```bash
docker compose -f docker-compose.prod.yml restart api
```

### Base de datos no conecta:
```bash
docker compose -f docker-compose.prod.yml restart postgres
```

### Limpiar todo y reiniciar:
```bash
./scripts/cleanup.sh --force
./scripts/start.sh --build-worker
```

### Ver logs completos:
```bash
docker compose -f docker-compose.prod.yml logs > all_logs.txt
```

---

## 📚 Referencias

- **GETTING_STARTED.md**: Documentación principal
- **GETTING_STARTED_KUBERNETES.md**: Guía específica para Kubernetes
- **DEVELOPMENT.md**: Documentación para desarrolladores
