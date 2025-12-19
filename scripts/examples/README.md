# Hodei Job Examples

Este directorio contiene scripts de ejemplo para demostrar las capacidades del Hodei Job Platform.

## Scripts Disponibles

### 1. **hello-world** - Demo Básico
```bash
just job-hello-world
```
Un job simple que demuestra la ejecución básica con pasos claros y output formateado.

### 2. **data-processing** - Pipeline ETL
```bash
just job-data-processing
```
Simula un pipeline de procesamiento de datos con:
- Ingesta de datos (10,000 registros)
- Transformación en batches
- Validación
- Generación de outputs (CSV, JSON, Parquet)

### 3. **ml-training** - Machine Learning
```bash
just job-ml-training
```
Entrena un modelo de ML simulado:
- 10 epochs de entrenamiento
- Métricas de accuracy
- Validación en test set
- Export del modelo

### 4. **cicd-build** - CI/CD Pipeline
```bash
just job-cicd-build
```
Simula un pipeline CI/CD completo:
- Checkout y setup
- Instalación de dependencias
- Linting y type checking
- Tests unitarios e integración
- Build y package
- Upload de artifacts

### 5. **cpu-stress** - Stress Test
```bash
just job-cpu-stress
```
Prueba de stress del sistema:
- CPU al 100% (30 segundos)
- Allocación de memoria (1 GB)
- I/O de disco (500 MB)

### 6. **error-handling** - Demo de Errores
```bash
just job-error-handling
```
Demuestra el manejo de errores:
- Operaciones exitosas
- Errores recuperables (con retry)
- Errores no recuperables (con cleanup)

## Uso con hodei-cli

### Ejecutar job y ver logs en tiempo real:
```bash
# Ejecutar job con logs en tiempo real (como kubectl logs)
cargo run --bin hodei-jobs-cli -- job run --name "My Job" --script scripts/examples/01-hello-world.sh

# En otra terminal, ver logs en tiempo real
cargo run --bin hodei-jobs-cli -- logs follow --job-id <JOB_ID>
```

### Flujo tipo kubectl:
```bash
# 1. Crear y ejecutar job (como kubectl apply)
cargo run --bin hodei-jobs-cli -- job run --name "demo" --command "echo 'Hello from Hodei!'"

# 2. Ver logs en tiempo real (como kubectl logs -f)
cargo run --bin hodei-jobs-cli -- logs follow --job-id <JOB_ID>

# 3. Ver estado del job
cargo run --bin hodei-jobs-cli -- job get --job-id <JOB_ID>

# 4. Listar todos los jobs
cargo run --bin hodei-jobs-cli -- job list
```

## Características

- ✅ **Log streaming en tiempo real** - Ve los logs mientras el job se ejecuta
- ✅ **Estado del job** - Seguimiento completo del lifecycle
- ✅ **Manejo de errores** - Jobs pueden fallar graciosamente
- ✅ **Recursos configurables** - CPU, memoria, timeout
- ✅ **Scripts reutilizables** - Fácil de crear nuevos jobs
