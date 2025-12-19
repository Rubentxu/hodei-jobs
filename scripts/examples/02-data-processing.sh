#!/bin/bash
set -euo pipefail

echo "📦 Data Processing Pipeline"
echo "==========================="
echo ""

# Generate mock data
echo "📊 Phase 1: Data Ingestion"
echo "   → Connecting to data source..."
sleep 1
echo "   ✓ Connected to source"
echo "   → Fetching 10,000 records..."
for i in {1..10}; do
    echo "   ⏳ Fetched $((i * 1000)) records..."
    sleep 0.3
done
echo "   ✓ 10,000 records fetched successfully"

echo ""
echo "📊 Phase 2: Data Transformation"
echo "   → Applying transformation rules..."
for i in {1..5}; do
    echo "   ⏳ Processing batch $i/5..."
    # Simulate CPU work
    for j in {1..1000000}; do echo "x" > /dev/null; done 2>/dev/null || true
    echo "   ✓ Batch $i processed (2,000 records)"
    sleep 0.5
done
echo "   ✓ 10,000 records transformed"

echo ""
echo "📊 Phase 3: Data Validation"
echo "   → Running validation checks..."
for rule in "Schema validation" "Null checks" "Data type checks" "Business rules"; do
    echo "   ✓ $rule passed"
    sleep 0.4
done
echo "   ✓ All validation checks passed"

echo ""
echo "📊 Phase 4: Output Generation"
echo "   → Generating output files..."
sleep 1
echo "   ✓ CSV file: data/output.csv (2.5 MB)"
echo "   ✓ JSON file: data/output.json (3.2 MB)"
echo "   ✓ Parquet file: data/output.parquet (1.8 MB)"

echo ""
echo "✅ Data Processing Pipeline Complete!"
echo "==========================="
echo "📈 Summary:"
echo "   - Input records: 10,000"
echo "   - Output records: 10,000"
echo "   - Success rate: 100%"
echo "   - Processing time: 15 seconds"
echo "   - Output size: 7.5 MB"
