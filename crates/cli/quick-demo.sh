#!/bin/bash
# Script de ejemplo rápido para probar la CLI

echo "🚀 Hodei CLI - Quick Start Demo"

# Verificar que la CLI esté construida
if [ ! -f "../../target/release/hodei-jobs-cli" ] && [ ! -f "../../target/debug/hodei-jobs-cli" ]; then
    echo "❌ CLI no encontrada. Construye primero: cargo build -p hodei-jobs-cli"
    exit 1
fi

# Determinar el binario CLI
CLI="../../target/release/hodei-jobs-cli"
if [ ! -f "$CLI" ]; then
    CLI="../../target/debug/hodei-jobs-cli"
fi

echo "✅ Usando CLI: $CLI"

# Test básico
echo ""
echo "📋 1. Consultando estado de la cola (puede fallar si el servidor no está ejecutándose):"
$CLI --server "http://localhost:50051" scheduler queue-status || echo "⚠️  Error esperado si el servidor no está ejecutándose"

echo ""
echo "🏃 2. Registrando worker de prueba:"
$CLI --server "http://localhost:50051" worker register --name "Worker Demo" --hostname "$(hostname)" || echo "⚠️  Error esperado si el servidor no está ejecutándose"

echo ""
echo "📊 3. Programando un job (scheduler schedule):"
$CLI --server "http://localhost:50051" scheduler schedule --name "Demo Scheduled Job" || echo "⚠️  Error esperado si el servidor no está ejecutándose"

echo ""
echo "✅ Demo básico completado"
echo "💡 Para el demo completo, ejecutar: ./demo.sh"
echo "📖 Para documentación: cat README.md"