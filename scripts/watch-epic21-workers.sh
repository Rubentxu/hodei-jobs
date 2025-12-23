#!/bin/bash
# Real-time EPIC-21 Worker Monitoring
# Watches Kubernetes and Docker for ephemeral worker lifecycle

set -e

NAMESPACE="${HODEI_K8S_NAMESPACE:-hodei-jobs-workers}"
DOCKER_LABEL="hodei-jobs-worker"
REFRESH_RATE=2

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
NC='\033[0m'

clear

echo -e "${BLUE}╔═══════════════════════════════════════════════════════════════╗${NC}"
echo -e "${BLUE}║      Real-time EPIC-21 Worker Monitor                        ║${NC}"
echo -e "${BLUE}║  Watching Kubernetes & Docker for Ephemeral Workers          ║${NC}"
echo -e "${BLUE}╚═══════════════════════════════════════════════════════════════╝${NC}"
echo ""
echo -e "${CYAN}Namespace:${NC} $NAMESPACE | ${CYAN}Refresh:${NC} ${REFRESH_RATE}s | ${CYAN}Label Filter:${NC} $DOCKER_LABEL"
echo ""

# Trap Ctrl+C
trap 'echo -e "\n${YELLOW}👋 Stopping monitor...${NC}"; exit 0' INT

while true; do
    # Clear screen except first lines
    echo -e "\033[H\033[3J"

    echo -e "${BLUE}╔═══════════════════════════════════════════════════════════════╗${NC}"
    echo -e "${BLUE}║      Real-time EPIC-21 Worker Monitor                        ║${NC}"
    echo -e "${BLUE}╚═══════════════════════════════════════════════════════════════╝${NC}"
    echo ""
    echo -e "${CYAN}🕐 $(date '+%Y-%m-%d %H:%M:%S')${NC}"
    echo ""

    # Kubernetes Section
    echo -e "${YELLOW}┌─ KUBERNETES WORKERS ───────────────────────────────────────┐${NC}"
    kubectl get pods -n "$NAMESPACE" --no-headers 2>/dev/null | while read line; do
        POD_NAME=$(echo "$line" | awk '{print $1}')
        STATUS=$(echo "$line" | awk '{print $3}')
        AGE=$(echo "$line" | awk '{print $5}')

        case "$STATUS" in
            Running)
                echo -e "${GREEN}✓${NC} $POD_NAME | ${GREEN}Running${NC} | $AGE"
                ;;
            Succeeded)
                echo -e "${BLUE}✓${NC} $POD_NAME | ${BLUE}Completed${NC} | $AGE"
                ;;
            Failed)
                echo -e "${RED}✗${NC} $POD_NAME | ${RED}Failed${NC} | $AGE"
                ;;
            Pending)
                echo -e "${YELLOW}⏳${NC} $POD_NAME | ${YELLOW}Pending${NC} | $AGE"
                ;;
            *)
                echo -e "  $POD_NAME | $STATUS | $AGE"
                ;;
        esac
    done
    echo -e "${YELLOW}└───────────────────────────────────────────────────────────────┘${NC}"

    echo ""

    # Docker Section
    echo -e "${YELLOW}┌─ DOCKER WORKERS ────────────────────────────────────────────┐${NC}"
    docker ps --filter "label=$DOCKER_LABEL" --format "table {{.Names}}\t{{.Status}}\t{{.CreatedAt}}" 2>/dev/null | tail -n +2 | while read line; do
        NAME=$(echo "$line" | awk '{print $1}')
        STATUS=$(echo "$line" | awk '{print $2}')
        CREATED=$(echo "$line" | awk '{print $3}')

        case "$STATUS" in
            Up*)
                echo -e "${GREEN}✓${NC} $NAME | ${GREEN}Running${NC} | $CREATED"
                ;;
            Exited*)
                EXIT_CODE=$(echo "$STATUS" | grep -oP 'Exited \(\K[0-9]+')
                if [[ "$EXIT_CODE" == "0" ]]; then
                    echo -e "${BLUE}✓${NC} $NAME | ${BLUE}Completed${NC} (exit $EXIT_CODE) | $CREATED"
                else
                    echo -e "${RED}✗${NC} $NAME | ${RED}Failed${NC} (exit $EXIT_CODE) | $CREATED"
                fi
                ;;
            *)
                echo -e "  $NAME | $STATUS | $CREATED"
                ;;
        esac
    done
    echo -e "${YELLOW}└───────────────────────────────────────────────────────────────┘${NC}"

    echo ""
    echo -e "${CYAN}Press Ctrl+C to exit${NC}"

    sleep $REFRESH_RATE
done
