#!/bin/bash
# security-check.sh

echo "=== INFORMACIÓN DEL SISTEMA ==="
uname -a
uptime

echo -e "\n=== PROCESOS SOSPECHOSOS ==="
ps aux --sort=-%cpu | head -10

echo -e "\n=== CONEXIONES DE RED ==="
ss -tuln | head -20

echo -e "\n=== ÚLTIMOS ACCESOS ==="
last | head -10

echo -e "\n=== USUARIOS CON ACCESO AL SISTEMA ==="
grep -E '/bin/(bash|sh|zsh)$' /etc/passwd

echo -e "\n=== ARCHIVOS CON PERMISOS SETUID/SETGID ==="
find / -perm -4000 -o -perm -2000 2>/dev/null | head -20

echo -e "\n=== SERVICIOS ACTIVOS ==="
systemctl list-units --type=service --state=running | head -20
