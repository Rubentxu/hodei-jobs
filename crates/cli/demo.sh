#!/bin/bash
# Demo script para mostrar el uso de la CLI del Hodei Job Platform

set -e

# Colores para output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

echo -e "${BLUE}======================================${NC}"
echo -e "${BLUE}   Hodei Job Platform - CLI Demo${NC}"
echo -e "${BLUE}======================================${NC}"
echo

# Verificar que la CLI esté construida
if [ ! -f "../../target/release/hodei-jobs-cli" ] && [ ! -f "../../target/debug/hodei-jobs-cli" ]; then
    echo -e "${RED}Error: CLI no encontrada. Construye el proyecto primero:${NC}"
    echo "cargo build -p hodei-jobs-cli"
    exit 1
fi

# Determinar el binario CLI
if [ -f "../../target/release/hodei-jobs-cli" ]; then
    CLI="../../target/release/hodei-jobs-cli"
else
    CLI="../../target/debug/hodei-jobs-cli"
fi

echo -e "${YELLOW}Usando CLI: $CLI${NC}"
echo

# Función para ejecutar comandos con delay
run_cmd() {
    local cmd="$1"
    local description="$2"
    
    echo -e "${BLUE}▶ $description${NC}"
    echo -e "${YELLOW}Ejecutando: $cmd${NC}"
    echo
    
    eval "$cmd"
    
    echo
    echo -e "${GREEN}✓ Completado${NC}"
    echo
    sleep 2
}

# Demo 1: Scheduler queue status
echo -e "${GREEN}=== DEMO 1: Scheduler ===${NC}"

run_cmd "$CLI --server 'http://localhost:50051' scheduler queue-status" \
    "Ver estado de la cola"

run_cmd "$CLI --server 'http://localhost:50051' scheduler schedule --name 'Demo Scheduled Job'" \
    "Programar un job (puede encolarse si no hay workers en el registry)"

run_cmd "$CLI --server 'http://localhost:50051' scheduler queue-status" \
    "Ver estado de la cola (después de programar)"

# Demo 2: Worker register/unregister
echo -e "${GREEN}=== DEMO 2: Worker Agent ===${NC}"

run_cmd "$CLI --server 'http://localhost:50051' worker register --name 'Worker Demo' --hostname '$(hostname)'" \
    "Registrar worker (WorkerAgentService/Register)"

run_cmd "$CLI --server 'http://localhost:50051' worker unregister --id 'worker-demo'" \
    "Intentar desregistrar worker (puede fallar si el ID no existe)"

# Demo 3: Job queue
echo -e "${GREEN}=== DEMO 3: JobExecution ===${NC}"

run_cmd "$CLI --server 'http://localhost:50051' job queue --name 'Demo Job (CLI)' --command 'echo'" \
    "Encolar job (JobExecutionService/QueueJob)"

echo
echo -e "${BLUE}======================================${NC}"
echo -e "${GREEN}   Demo completado exitosamente!${NC}"
echo -e "${BLUE}======================================${NC}"
echo
echo -e "${YELLOW}Para más información:${NC}"
echo "- Ver README.md para documentación completa"
echo "- Usar --help en cualquier comando para ayuda detallada"
echo "- Probar diferentes combinaciones de filtros y opciones"
echo