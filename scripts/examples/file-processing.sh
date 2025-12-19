#!/bin/bash
# =============================================================================
# File Processing Pipeline - Self-contained ETL simulation
# =============================================================================
# Simulates data processing: generate, transform, validate, aggregate

set -e

echo "╔════════════════════════════════════════════════════════════════╗"
echo "║           FILE PROCESSING PIPELINE - ETL Simulation            ║"
echo "╚════════════════════════════════════════════════════════════════╝"
echo ""
echo "📅 Started: $(date '+%Y-%m-%d %H:%M:%S')"
echo ""

WORK_DIR="/tmp/etl_$$"
mkdir -p "$WORK_DIR"
trap "rm -rf $WORK_DIR" EXIT

RECORDS=1000

echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "[1/6] Generating source data..."
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

echo "  → Creating $RECORDS records..."
echo "id,timestamp,user_id,action,value,status" > "$WORK_DIR/source.csv"

for i in $(seq 1 $RECORDS); do
    ts=$(date -d "-$((RANDOM % 86400)) seconds" '+%Y-%m-%d %H:%M:%S' 2>/dev/null || date '+%Y-%m-%d %H:%M:%S')
    user_id=$((RANDOM % 1000 + 1))
    actions=("login" "purchase" "view" "logout" "search")
    action=${actions[$((RANDOM % 5))]}
    value=$((RANDOM % 10000))
    statuses=("success" "success" "success" "failed" "pending")
    status=${statuses[$((RANDOM % 5))]}
    echo "$i,$ts,$user_id,$action,$value,$status" >> "$WORK_DIR/source.csv"
    
    if [ $((i % 200)) -eq 0 ]; then
        echo "    Progress: $i/$RECORDS records generated"
    fi
done

SIZE=$(wc -c < "$WORK_DIR/source.csv")
echo "  ✓ Generated source.csv ($SIZE bytes, $RECORDS records)"
echo ""

echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "[2/6] Validating data quality..."
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
sleep 1

TOTAL=$(tail -n +2 "$WORK_DIR/source.csv" | wc -l)
EMPTY=$(tail -n +2 "$WORK_DIR/source.csv" | awk -F',' 'NF<6' | wc -l)
VALID=$((TOTAL - EMPTY))

echo "  Total records:    $TOTAL"
echo "  Valid records:    $VALID"
echo "  Invalid records:  $EMPTY"
echo "  Data quality:     $(echo "scale=1; $VALID * 100 / $TOTAL" | bc)%"
echo ""

echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "[3/6] Transforming data..."
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

echo "  → Filtering successful transactions..."
head -1 "$WORK_DIR/source.csv" > "$WORK_DIR/success.csv"
grep ",success$" "$WORK_DIR/source.csv" >> "$WORK_DIR/success.csv"
SUCCESS_COUNT=$(tail -n +2 "$WORK_DIR/success.csv" | wc -l)
echo "    ✓ $SUCCESS_COUNT successful records"

echo "  → Extracting purchase events..."
head -1 "$WORK_DIR/source.csv" > "$WORK_DIR/purchases.csv"
grep ",purchase," "$WORK_DIR/source.csv" >> "$WORK_DIR/purchases.csv"
PURCHASE_COUNT=$(tail -n +2 "$WORK_DIR/purchases.csv" | wc -l)
echo "    ✓ $PURCHASE_COUNT purchase records"

echo "  → Normalizing values..."
awk -F',' 'NR==1 {print $0",normalized_value"} NR>1 {printf "%s,%.2f\n", $0, $5/100}' \
    "$WORK_DIR/source.csv" > "$WORK_DIR/normalized.csv"
echo "    ✓ Values normalized"
echo ""

echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "[4/6] Aggregating statistics..."
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
sleep 1

echo "  Action Distribution:"
tail -n +2 "$WORK_DIR/source.csv" | cut -d',' -f4 | sort | uniq -c | sort -rn | while read count action; do
    pct=$(echo "scale=1; $count * 100 / $TOTAL" | bc)
    printf "    %-12s %5d (%5.1f%%)\n" "$action" "$count" "$pct"
done

echo ""
echo "  Status Distribution:"
tail -n +2 "$WORK_DIR/source.csv" | cut -d',' -f6 | sort | uniq -c | sort -rn | while read count status; do
    pct=$(echo "scale=1; $count * 100 / $TOTAL" | bc)
    printf "    %-12s %5d (%5.1f%%)\n" "$status" "$count" "$pct"
done

TOTAL_VALUE=$(tail -n +2 "$WORK_DIR/source.csv" | cut -d',' -f5 | paste -sd+ | bc)
AVG_VALUE=$(echo "scale=2; $TOTAL_VALUE / $TOTAL" | bc)
echo ""
echo "  Value Statistics:"
echo "    Total value:   $TOTAL_VALUE"
echo "    Average value: $AVG_VALUE"
echo ""

echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "[5/6] Generating reports..."
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

cat > "$WORK_DIR/report.json" << EOF
{
  "report_date": "$(date '+%Y-%m-%d %H:%M:%S')",
  "total_records": $TOTAL,
  "valid_records": $VALID,
  "success_count": $SUCCESS_COUNT,
  "purchase_count": $PURCHASE_COUNT,
  "total_value": $TOTAL_VALUE,
  "average_value": $AVG_VALUE,
  "status": "completed"
}
EOF

echo "  ✓ JSON report generated"
echo "  ✓ CSV exports ready"
echo ""

echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "[6/6] Cleanup and summary..."
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

TOTAL_SIZE=$(du -sh "$WORK_DIR" | cut -f1)
FILE_COUNT=$(ls -1 "$WORK_DIR" | wc -l)
echo "  Files created: $FILE_COUNT"
echo "  Total size: $TOTAL_SIZE"
echo ""

echo "╔════════════════════════════════════════════════════════════════╗"
echo "║                  PIPELINE COMPLETED                            ║"
echo "╠════════════════════════════════════════════════════════════════╣"
echo "║  Records Processed: $TOTAL                                    ║"
echo "║  Success Rate:      $(echo "scale=1; $SUCCESS_COUNT * 100 / $TOTAL" | bc)%                                   ║"
echo "║  Status:            ✅ SUCCESS                                 ║"
echo "╚════════════════════════════════════════════════════════════════╝"
